// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::Export;
use crate::{
    client::{ElasticsearchBuilder, ElasticsearchClient},
    data::{self, Auth, KnownHost},
    processor::{BatchResponse, DiagnosticReport},
};
use elasticsearch::{
    BulkOperation, BulkParts, IndexParts,
    http::{Method, headers, request::JsonBody, response::Response},
};
use eyre::{Result, eyre};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::time::{Duration, timeout};
use url::Url;

/// An exporter that sends documents to an Elasticsearch cluster.
#[derive(Clone)]
pub struct ElasticsearchExporter {
    client: ElasticsearchClient,
    tx_limit: Arc<Semaphore>,
    docs_tx: Option<mpsc::Sender<usize>>,
    requires_secret: bool,
    url: Url,
}

impl ElasticsearchExporter {
    fn request_timeout() -> Duration {
        Duration::from_millis(
            std::env::var("ESDIAG_REQUEST_TIMEOUT_MS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(30_000),
        )
    }

    /// Create a new ElasticsearchExporter from a URL and Auth
    pub fn try_new(url: Url, auth: Auth) -> Result<Self> {
        let requires_secret = !matches!(auth, Auth::None);
        let client = ElasticsearchBuilder::new(url.clone())
            .insecure(true)
            .auth(auth)
            .build()?;

        let limit = std::env::var("ESDIAG_OUTPUT_TASK_LIMIT")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(10);

        log::info!("Elasticsearch task limit set to {}", limit);

        Ok(Self {
            client,
            tx_limit: Arc::new(Semaphore::new(limit)),
            url,
            docs_tx: None,
            requires_secret,
        })
    }

    pub fn requires_secret(&self) -> bool {
        self.requires_secret
    }

    /// Request to an arbitrary path on the Elasticsearch client
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        value: Option<&Value>,
    ) -> Result<Response> {
        let method = match method {
            "POST" => Method::Post,
            "PUT" => Method::Put,
            "DELETE" => Method::Delete,
            _ => Method::Get,
        };
        let body = value.map(JsonBody::new);
        timeout(
            Self::request_timeout(),
            self.client.send(
                method,
                path,
                headers::HeaderMap::new(),
                Option::<&Value>::None,
                body,
                None,
            ),
        )
        .await
        .map_err(|_| eyre!("Request timeout for {method:?} {path}"))?
        .map_err(|e| e.into())
    }
}

impl TryFrom<KnownHost> for ElasticsearchExporter {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        let requires_secret = !matches!(host.get_auth()?, Auth::None);
        let url = host.get_url();
        let client = ElasticsearchClient::try_from(host)?;
        let limit = std::env::var("ESDIAG_OUTPUT_TASK_LIMIT")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(10);

        log::debug!("Elasticsearch output task limit: {}", limit);

        Ok(Self {
            client,
            tx_limit: Arc::new(Semaphore::new(limit)),
            url,
            docs_tx: None,
            requires_secret,
        })
    }
}

impl Export for ElasticsearchExporter {
    fn get_docs_rx(&mut self) -> mpsc::Receiver<usize> {
        let (tx, rx) = mpsc::channel::<usize>(100);
        self.docs_tx = Some(tx);
        rx
    }

    /// Check if the exporter has a valid connection to Elasticsearch.
    async fn is_connected(&self) -> bool {
        let status_code = match timeout(Self::request_timeout(), self.client.info().send()).await {
            Ok(Ok(res)) => {
                log::debug!("Exporter is connected: {}", res.status_code());
                log::trace!("{:?}", res);
                res.status_code().as_u16()
            }
            Ok(Err(e)) => {
                log::error!("{e}");
                599
            }
            Err(_) => {
                log::error!(
                    "Timed out checking exporter connection after {:?}",
                    Self::request_timeout()
                );
                599
            }
        };

        status_code == 200
    }

    /// Sends a single batch of documents directly to Elasticsearch with backpressure.
    /// Returns a BatchResponse directly without spawning tasks.
    async fn batch_send<T>(&self, index: String, docs: Vec<T>) -> Result<BatchResponse>
    where
        T: Serialize + Sized + Send + Sync,
    {
        let batch: Vec<BulkOperation<T>> = docs
            .into_iter()
            .map(|doc| BulkOperation::create(doc).pipeline("esdiag").into())
            .collect();

        let response = timeout(
            Self::request_timeout(),
            self.client
                .bulk(BulkParts::Index(&index))
                .body(batch)
                .send(),
        )
        .await
        .map_err(|_| {
            eyre!(
                "Timed out sending bulk request to {} for index {}",
                self.url,
                index
            )
        })?;

        parse_response(index, response).await
    }

    /// Transmits a single batch of documents with semaphore-based connection limiting
    /// Returns a one-shot channel for the BatchResponse
    async fn batch_tx<T>(
        &self,
        index: String,
        docs: Vec<T>,
    ) -> Result<oneshot::Receiver<BatchResponse>>
    where
        T: Serialize + Sized + Send + Sync + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let client = self.client.clone();
        let semaphore = self.tx_limit.clone();
        let docs_tx = self.docs_tx.clone();
        let doc_count = docs.len();

        tokio::spawn(async move {
            // Acquire semaphore permit inside task - blocks if at limit (backpressure)
            let _permit = semaphore
                .acquire()
                .await
                .expect("Failed to acquire semaphore permit");

            let batch: Vec<BulkOperation<T>> = docs
                .into_iter()
                .map(|doc| BulkOperation::create(doc).pipeline("esdiag").into())
                .collect();

            let response = timeout(
                ElasticsearchExporter::request_timeout(),
                client.bulk(BulkParts::Index(&index)).body(batch).send(),
            )
            .await;

            let parsed = match response {
                Ok(response) => parse_response(index, response).await,
                Err(_) => Err(eyre!(
                    "Timed out sending bulk request after {:?}",
                    ElasticsearchExporter::request_timeout()
                )),
            };

            match parsed {
                Ok(batch_response) => {
                    if tx.send(batch_response).is_err() {
                        log::error!("Failed to send batch response: receiver dropped");
                    } else if let Some(tx) = docs_tx {
                        let _ = tx.send(doc_count).await;
                    }
                }
                Err(e) => {
                    log::warn!("Bulk batch failed: {}", e);
                }
            }
        });

        Ok(rx)
    }

    /// Sends the final diagnostic report document to Elasticsearch.
    async fn save_report(&self, report: &DiagnosticReport) -> Result<()> {
        data::save_file("report.json", report)?;
        let diagnostic_id = report.diagnostic.metadata.id.clone();
        match timeout(
            Self::request_timeout(),
            self.client
                .index(IndexParts::Index("metrics-diagnostic-esdiag"))
                .pipeline("esdiag")
                .body(&report)
                .send(),
        )
        .await
        {
            Err(_) => Err(eyre!(
                "Timed out saving report {} after {:?}",
                diagnostic_id,
                Self::request_timeout()
            )),
            Ok(res) => match res {
                Ok(res) => {
                    let status_code = res.status_code().as_u16();
                    let body = res.json::<Value>().await?;
                    match status_code {
                        200 | 201 => {
                            log::info!(
                                "metrics-diagnostic-esdiag, created diagnostic report {}",
                                diagnostic_id
                            );
                            log::trace!("response body: {body}");
                            Ok(())
                        }
                        400..600 => Err(eyre!("http {status_code}: {body}")),
                        _ => Err(eyre!("unexpected response: http {status_code}: {body}")),
                    }
                }
                Err(e) => {
                    log::error!("{e}");
                    Err(e.into())
                }
            },
        }
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
) -> Result<BatchResponse> {
    let response = response?;
    log::trace!("{:?}", &response);
    let status_code = response.status_code().as_u16();
    let body: Value = response.json().await?;
    let mut items: Vec<Value> = body["items"].as_array().unwrap_or(&Vec::new()).clone();
    let item_count = items.len();

    let error_items: Vec<Value> = items
        .drain(..)
        .filter(|item| match item["create"]["status"].as_u64() {
            Some(status) => status != 201,
            None => false,
        })
        .collect();
    let error_count = error_items.len();
    let doc_count = item_count - error_count;

    if (status_code != 200 && log::max_level() >= log::Level::Debug)
        || (log::max_level() >= log::Level::Trace)
    {
        data::save_file(
            "responses.ndjson",
            &serde_json::json!({
                "index": index,
                "doc_count": doc_count,
                "error_count": error_count,
                "errors": error_items,
                "body": body,
            }),
        )?;
    }

    match status_code {
        200 if error_count == 0 => log::debug!("{}, created {} docs", index, doc_count),
        200 => log::warn!(
            "{}, created {} docs with {} errors",
            index,
            doc_count,
            error_count
        ),
        400 => return Err(eyre!("{} - http 400 bad request", index)),
        401 => return Err(eyre!("{} - http 401 unauthorized", index)),
        403 => return Err(eyre!("{} - http 403 forbidden", index)),
        404 => return Err(eyre!("{} - http 404 not found", index)),
        413 => return Err(eyre!("{} - http 413 request too large", index)),
        429 => return Err(eyre!("{} - http 429 too many requests", index)),
        500..=599 => return Err(eyre!("{} - server errors: http {}", status_code, index)),
        _ => log::warn!("unexpected http response: {}", status_code),
    }

    if error_count > 0 {
        data::save_file(
            "bulk_errors.ndjson",
            &serde_json::json!({
                "index": index,
                "doc_count": doc_count,
                "error_count": error_count,
                "errors": error_items
            }),
        )?;
    }

    let batch_response = BatchResponse {
        docs: item_count as u32,
        errors: error_count as u32,
        retries: 0,
        size: 0,
        status_code,
        time: body.get("took").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
    };

    Ok(batch_response)
}
