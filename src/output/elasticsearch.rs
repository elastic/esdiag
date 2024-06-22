use crate::{host::Host, output::file};
use elasticsearch::{
    auth::Credentials,
    http::{
        headers,
        request::JsonBody,
        response::Response,
        transport::{SingleNodeConnectionPool, TransportBuilder},
        Method,
    },
    BulkOperation, BulkParts, Elasticsearch, Error,
};
use futures::{future::join_all, stream::FuturesUnordered};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Semaphore;
use url::Url;

const BULK_SIZE: usize = 5_000;
const ES_WORKERS: usize = 4;

#[derive(Clone, Debug)]
pub struct ElasticsearchClient {
    client: Elasticsearch,
}

impl ElasticsearchClient {
    pub fn new(host: Host) -> Self {
        match host {
            Host::ApiKey {
                url,
                apikey,
                cloud_id,
                ..
            } => Self::new_apikey(url, apikey, cloud_id),
            Host::Basic {
                url,
                username,
                password,
                cloud_id,
                ..
            } => Self::new_basic(url, username, password, cloud_id),
            Host::None { url, .. } => Self::new_none(url),
        }
    }

    fn new_none(url: Url) -> Self {
        // Create a connection pool with the Elasticsearch server URL
        let connection_pool = SingleNodeConnectionPool::new(url);

        // Create a transport builder with the connection pool
        let transport = match TransportBuilder::new(connection_pool).build() {
            Ok(transport) => transport,
            Err(why) => {
                log::error!("Failed to create transport: {:?}", why);
                std::process::exit(1);
            }
        };

        // Create an Elasticsearch client with the transport
        let client = Elasticsearch::new(transport);

        Self { client }
    }

    fn new_basic(url: Url, username: String, password: String, _cloud_id: Option<String>) -> Self {
        // Create a connection pool with the Elasticsearch server URL
        let connection_pool = SingleNodeConnectionPool::new(url);

        // Create a transport builder with the connection pool
        let transport = match TransportBuilder::new(connection_pool)
            .auth(Credentials::Basic(username, password))
            .build()
        {
            Ok(transport) => transport,
            Err(why) => {
                log::error!("Failed to create transport: {:?}", why);
                std::process::exit(1);
            }
        };

        // Create an Elasticsearch client with the transport
        let client = Elasticsearch::new(transport);

        Self { client }
    }

    fn new_apikey(url: Url, apikey: String, cloud_id: Option<String>) -> Self {
        let transport = match cloud_id {
            Some(_cloud_id) => {
                // When using cloud_id I couldn't get the apikey to work ¯\_(ツ)_/¯
                log::debug!("Cloud ID provided, but not used: {_cloud_id}");
                let connection_pool = SingleNodeConnectionPool::new(url);
                TransportBuilder::new(connection_pool)
                    .header(
                        headers::AUTHORIZATION,
                        format!("ApiKey {}", apikey)
                            .parse()
                            .expect("Failed to parse apikey"),
                    )
                    .build()
                    .ok()
            }
            None => {
                let connection_pool = SingleNodeConnectionPool::new(url);
                TransportBuilder::new(connection_pool)
                    .header(headers::ACCEPT_ENCODING, "gzip".parse().unwrap())
                    .header(
                        headers::AUTHORIZATION,
                        format!("ApiKey {}", apikey)
                            .parse()
                            .expect("Failed to parse apikey"),
                    )
                    .build()
                    .ok()
            }
        };

        let client = match transport {
            Some(transport) => Elasticsearch::new(transport),
            None => {
                log::error!("Failed to create Elasticsearch transport");
                std::process::exit(1);
            }
        };

        log::debug!("Elasticsearch client: {:?}", client);
        Self { client }
    }

    pub async fn send_asset(
        &self,
        path: &str,
        value: &Option<Value>,
        method: &str,
    ) -> Result<Response, Error> {
        let method = match method {
            "POST" => Method::Post,
            "PUT" => Method::Put,
            "DELETE" => Method::Delete,
            _ => Method::Get,
        };
        let body = match value {
            Some(value) => Some(JsonBody::new(value)),
            None => None,
        };
        self.client
            .send(
                method,
                path,
                headers::HeaderMap::new(),
                Option::<&Value>::None,
                body,
                None,
            )
            .await
    }

    pub async fn test(&self) -> Result<Response, Error> {
        log::debug!("Testing client {:?}", self.client);
        self.client
            .send(
                Method::Get,
                "",
                headers::HeaderMap::new(),
                Option::<&String>::None,
                Option::<&String>::None,
                None,
            )
            .await
    }

    pub async fn bulk_index(&self, mut docs: Vec<Value>) -> std::io::Result<usize> {
        let workers = ES_WORKERS;
        let semaphore = Arc::new(Semaphore::new(workers));
        let index = format!(
            "{}-{}-{}",
            docs[0]["data_stream"]["type"].as_str().unwrap(),
            docs[0]["data_stream"]["dataset"].as_str().unwrap(),
            docs[0]["data_stream"]["namespace"].as_str().unwrap()
        );

        let futures = FuturesUnordered::new();

        while !docs.is_empty() {
            let mut ops: Vec<BulkOperation<Value>> = Vec::new();
            for _ in 0..BULK_SIZE {
                if let Some(doc) = docs.pop() {
                    ops.push(BulkOperation::create(doc).pipeline("esdiag").into());
                } else {
                    break;
                }
            }

            let client = self.clone();
            let index = index.clone();
            let semaphore = semaphore.clone();
            let future = async move {
                let _permit = semaphore.acquire().await;
                client.bulk_index_batch(index, ops).await
            };

            futures.push(tokio::spawn(future));
        }

        // Await all futures to complete before returning
        let results = join_all(futures).await;
        let mut total_count = 0;
        for result in results {
            match result {
                Ok(count) => total_count += count.unwrap_or(0),
                Err(e) => {
                    log::error!("Failed to process bulk index result: {:?}", e);
                }
            }
        }
        Ok(total_count)
    }

    async fn bulk_index_batch(
        &self,
        index: String,
        ops: Vec<BulkOperation<Value>>,
    ) -> std::io::Result<usize> {
        // Index the batch
        let batch_size = &ops.len();
        match self
            .client
            .bulk(BulkParts::Index(&index))
            .body(ops)
            .send()
            .await
        {
            Ok(response) => {
                if response.status_code().is_success() {
                    match response.json::<Value>().await {
                        Ok(json) => {
                            match json["errors"].as_bool().unwrap_or(false) {
                                true => {
                                    let errors = json["items"]
                                        .as_array()
                                        .unwrap()
                                        .iter()
                                        .filter(|item| {
                                            item["create"]["status"].as_i64().unwrap_or(0) >= 400
                                        })
                                        .map(|item| item["create"].clone())
                                        .collect::<Vec<Value>>();
                                    let error_count = errors.len();
                                    file::write_ndjson_if_debug(
                                        Value::from(errors),
                                        "errors.ndjson",
                                        true,
                                    )
                                    .ok();
                                    log::warn!(
                                        "{} indexed {} documents with {} errors",
                                        index,
                                        batch_size - error_count,
                                        error_count
                                    );
                                }
                                false => {
                                    log::info!("{} indexed {} documents", index, batch_size);
                                }
                            }
                            file::write_ndjson_if_debug(json, "responses.ndjson", true).ok();
                        }
                        Err(e) => {
                            log::error!("Failed to parse response: {:?}", &e);
                        }
                    };
                    Ok(*batch_size)
                } else {
                    log::error!("Failed to index document to {}: {:?}", index, response);
                    let body = match response.json::<Value>().await {
                        Ok(json) => {
                            log::error!("{:?}", json);
                        }
                        Err(e) => {
                            log::error!("Failed to parse response: {:?}", e);
                        }
                    };
                    log::error!("{:?}", body);
                    Ok(*batch_size)
                }
            }
            Err(e) => {
                log::error!("Failed to index document to {}: {:?}", index, e);
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to index document into {index}"),
                ))
            }
        }
    }
}
