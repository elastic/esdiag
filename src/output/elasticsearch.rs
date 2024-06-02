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
use serde_json::Value;
use url::Url;

#[derive(Debug)]
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
                        format!("ApiKey {}", apikey).parse().unwrap(),
                    )
                    .build()
                    .ok()
            }
            None => {
                let connection_pool = SingleNodeConnectionPool::new(url);
                TransportBuilder::new(connection_pool)
                    .header(
                        headers::AUTHORIZATION,
                        format!("ApiKey {}", apikey).parse().unwrap(),
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

    pub async fn bulk_index(&self, mut docs: Vec<Value>) -> Result<String, String> {
        let index = format!(
            "{}-{}-{}",
            docs[0]["data_stream"]["type"].as_str().unwrap(),
            docs[0]["data_stream"]["dataset"].as_str().unwrap(),
            docs[0]["data_stream"]["namespace"].as_str().unwrap()
        );

        while docs.len() > 0 {
            let mut ops: Vec<BulkOperation<Value>> = Vec::new();
            for _ in 0..10_000 {
                let doc = match docs.pop() {
                    Some(doc) => doc,
                    None => break,
                };
                ops.push(BulkOperation::create(doc).pipeline("esdiag").into());
            }
            self.bulk_index_batch(&index, ops).await?;
        }

        Ok("Indexed documents".to_string())
    }

    async fn bulk_index_batch(
        &self,
        index: &str,
        ops: Vec<BulkOperation<Value>>,
    ) -> Result<String, String> {
        // Index the batch
        let batch_size = &ops.len();
        match self
            .client
            .bulk(BulkParts::Index(index))
            .body(ops)
            .send()
            .await
        {
            Ok(response) => {
                if response.status_code().is_success() {
                    log::info!("{}: indexed {} documents", batch_size, index);
                    let status = response.status_code().to_string().clone();
                    match response.json::<Value>().await {
                        Ok(json) => {
                            file::write_ndjson_if_debug(index, json, "responses.ndjson").ok();
                            //println!("{}", &json);
                        }
                        Err(why) => {
                            log::error!("Failed to parse response: {:?}", &why);
                        }
                    };
                    Ok(status)
                } else {
                    log::error!("Failed to index document to {}: {:?}", index, response);
                    let status = response.status_code().to_string().clone();
                    let body = match response.json::<Value>().await {
                        Ok(json) => {
                            log::error!("{:?}", json);
                        }
                        Err(why) => {
                            log::error!("Failed to parse response: {:?}", why);
                        }
                    };
                    log::error!("{:?}", body);
                    Ok(status)
                }
            }
            Err(why) => {
                log::error!("Failed to index document to {}: {:?}", index, why);
                Err(format!("Failed to index document into {index}"))
            }
        }
    }
}
