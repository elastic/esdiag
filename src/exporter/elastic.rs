use super::Export;
use crate::client::{Auth, ElasticsearchBuilder, Host};
use color_eyre::eyre::{eyre, Result};
use elasticsearch::{
    http::{headers, request::JsonBody, response::Response, Method},
    BulkOperation, BulkParts, Elasticsearch,
};
use futures::{future::join_all, stream::FuturesUnordered};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Semaphore;
use url::Url;

pub struct ElasticsearchExporter {
    client: Elasticsearch,
    url: Url,
}

impl ElasticsearchExporter {
    /// Create a new ElasticsearchExporter from a URL and Auth
    pub fn new(url: Url, auth: Auth) -> Result<Self> {
        let client = ElasticsearchBuilder::new(url.clone())
            .insecure(true)
            .auth(auth)
            .build()?;

        Ok(Self { client, url })
    }

    /// Send a request to an arbitrary path on the Elasticsearch client
    pub async fn send(&self, method: &str, path: &str, value: Option<&Value>) -> Result<Response> {
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
            .map_err(|e| e.into())
    }
}

impl TryFrom<Host> for ElasticsearchExporter {
    type Error = color_eyre::eyre::Report;

    fn try_from(host: Host) -> Result<Self> {
        let url = host.get_url();
        let client = Elasticsearch::try_from(host)?;
        Ok(Self { client, url })
    }
}

impl Export for ElasticsearchExporter {
    async fn write(&self, index: String, mut docs: Vec<Value>) -> Result<usize> {
        let client = Arc::new(self.client.clone());
        let workers = 4;
        let bulk_size = 5000;
        let semaphore = Arc::new(Semaphore::new(workers));

        let futures = FuturesUnordered::new();

        while !docs.is_empty() {
            let client = client.clone();
            let index = index.clone();
            let batch_size = std::cmp::min(docs.len(), bulk_size);
            let ops: Vec<BulkOperation<serde_json::Value>> = docs
                .drain(..batch_size)
                .map(|doc| BulkOperation::create(doc).pipeline("esdiag").into())
                .collect();
            let semaphore = semaphore.clone();
            let future = async move {
                let _permit = semaphore.acquire().await;
                let response = client.bulk(BulkParts::Index(&index)).body(ops).send().await;
                parse_response(index, response).await
            };

            futures.push(tokio::spawn(future));
        }
        let doc_count = join_all(futures)
            .await
            .into_iter()
            .filter_map(Result::ok)
            .filter_map(|result| match result {
                Ok(count) => Some(count),
                Err(e) => {
                    log::error!("{}", e);
                    None
                }
            })
            .sum();

        Ok(doc_count)
    }

    async fn is_connected(&self) -> bool {
        let status_code = match self
            .client
            .send(
                elasticsearch::http::Method::Get,
                "",
                elasticsearch::http::headers::HeaderMap::new(),
                Option::<&String>::None,
                Option::<&String>::None,
                None,
            )
            .await
        {
            Ok(res) => {
                log::trace!("{:?}", res);
                res.status_code().as_str().to_string()
            }
            Err(e) => {
                log::error!("{e}");
                "599".to_string()
            }
        };

        status_code == "200"
    }
}

impl std::fmt::Display for ElasticsearchExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}

async fn parse_response(
    index: String,
    response: Result<Response, elasticsearch::Error>,
) -> Result<usize> {
    let response = response?;
    log::trace!("{:?}", &response);
    let status_code = response.status_code().as_u16();
    let body: Value = response.json().await?;
    let mut items: Vec<Value> = body["items"].as_array().unwrap_or(&Vec::new()).clone();
    let item_count = items.len();

    let error_items: Vec<Value> = items
        .drain(..)
        .filter(|item| match item["create"]["status"].as_u64() {
            Some(s) => s != 201,
            None => false,
        })
        .collect();
    let error_count = error_items.len();
    let doc_count = item_count - error_count;

    match status_code {
        200 if error_count == 0 => log::info!("{}, wrote {} docs", index, doc_count),
        200 => log::warn!(
            "{}, wrote {} docs with {} errors",
            index,
            doc_count,
            error_count
        ),
        401 => return Err(eyre!("{} - http 401 unauthorized", index)),
        403 => return Err(eyre!("{} - http 403 forbidden", index)),
        404 => return Err(eyre!("{} - http 404 not found", index)),
        413 => return Err(eyre!("{} - http 413 request too large", index)),
        429 => return Err(eyre!("{} - http 429 too many requests", index)),
        500..=599 => return Err(eyre!("{} - server errors: http {}", status_code, index)),
        _ => log::warn!("unexpected http response: {}", status_code),
    }

    if log::max_level() >= log::Level::Debug {
        println!("{}", serde_json::json!({"index":index}));
        println!("{}", serde_json::json!(error_items));
    }

    Ok(doc_count)
}
