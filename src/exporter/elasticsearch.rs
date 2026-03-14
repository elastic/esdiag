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

#[derive(Debug)]
enum ExporterError {
    RateLimited,
    Fatal(eyre::Report),
}

impl std::fmt::Display for ExporterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExporterError::RateLimited => write!(f, "http 429 too many requests"),
            ExporterError::Fatal(e) => write!(f, "{e}"),
        }
    }
}

struct RetryConfig {
    max_retries: u16,
    initial_ms: u64,
    max_ms: u64,
}

impl RetryConfig {
    fn from_env() -> Self {
        Self {
            max_retries: std::env::var("ESDIAG_EXPORT_RETRY_MAX")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            initial_ms: std::env::var("ESDIAG_EXPORT_RETRY_INITIAL_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1_000),
            max_ms: std::env::var("ESDIAG_EXPORT_RETRY_MAX_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30_000),
        }
    }
}

fn backoff_ms(attempt: u16, config: &RetryConfig) -> u64 {
    let base = config.initial_ms.saturating_mul(1u64 << u32::from(attempt).min(30));
    let jitter = 0.75 + rand::random::<f64>() * 0.5;
    let jittered = (base as f64 * jitter) as u64;
    jittered.min(config.max_ms)
}

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

        tracing::info!("Elasticsearch task limit set to {}", limit);

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

        tracing::debug!("Elasticsearch output task limit: {}", limit);

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
                tracing::debug!("Exporter is connected: {}", res.status_code());
                tracing::trace!("{:?}", res);
                res.status_code().as_u16()
            }
            Ok(Err(e)) => {
                tracing::error!("{e}");
                599
            }
            Err(_) => {
                tracing::error!(
                    "Timed out checking exporter connection after {:?}",
                    Self::request_timeout()
                );
                599
            }
        };

        status_code == 200
    }

    /// Sends a single batch of documents to Elasticsearch, retrying on HTTP 429
    /// with exponential backoff. Serialises docs to Values upfront so the batch
    /// can be resent without requiring `T: Clone`.
    async fn batch_send<T>(&self, index: String, docs: Vec<T>) -> Result<BatchResponse>
    where
        T: Serialize + Sized + Send + Sync,
    {
        let config = RetryConfig::from_env();

        let values: Vec<Arc<Value>> = docs
            .into_iter()
            .map(|doc| serde_json::to_value(doc).map(Arc::new))
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| eyre!("Failed to serialize document: {e}"))?;

        let mut retries: u16 = 0;

        for attempt in 0..=config.max_retries {
            let batch: Vec<BulkOperation<Arc<Value>>> = values
                .iter()
                .map(|doc| BulkOperation::create(Arc::clone(doc)).pipeline("esdiag").into())
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

            match parse_response(index.clone(), response, retries).await {
                Ok(batch_response) => return Ok(batch_response),
                Err(ExporterError::RateLimited) if attempt < config.max_retries => {
                    let sleep_ms = backoff_ms(attempt, &config);
                    tracing::warn!(
                        "{index} - http 429, retry {}/{}, sleeping {sleep_ms}ms",
                        u32::from(attempt) + 1,
                        config.max_retries,
                    );
                    tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                    retries += 1;
                }
                Err(ExporterError::RateLimited) => {
                    tracing::error!(
                        "{index} - http 429, batch dropped after {} attempts",
                        u32::from(config.max_retries) + 1,
                    );
                    return Ok(BatchResponse {
                        docs: values.len() as u32,
                        errors: values.len() as u32,
                        retries,
                        size: 0,
                        status_code: 429,
                        time: 0,
                    });
                }
                Err(ExporterError::Fatal(e)) => return Err(e),
            }
        }

        unreachable!("retry loop always returns within attempt bounds")
    }

    /// Transmits a single batch of documents with semaphore-based connection limiting.
    /// Returns a one-shot channel for the BatchResponse.
    async fn batch_tx<T>(
        &self,
        index: String,
        docs: Vec<T>,
    ) -> Result<oneshot::Receiver<BatchResponse>>
    where
        T: Serialize + Sized + Send + Sync + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let exporter = self.clone();
        let semaphore = self.tx_limit.clone();
        let docs_tx = self.docs_tx.clone();
        let doc_count = docs.len();

        tokio::spawn(async move {
            // Acquire semaphore permit inside task - blocks if at limit (backpressure)
            let _permit = semaphore
                .acquire()
                .await
                .expect("Failed to acquire semaphore permit");

            match exporter.batch_send(index, docs).await {
                Ok(batch_response) => {
                    if tx.send(batch_response).is_err() {
                        tracing::error!("Failed to send batch response: receiver dropped");
                    } else if let Some(ch) = docs_tx {
                        let _ = ch.send(doc_count).await;
                    }
                }
                Err(e) => {
                    tracing::warn!("Bulk batch failed: {}", e);
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
                            tracing::info!(
                                "metrics-diagnostic-esdiag, created diagnostic report {}",
                                diagnostic_id
                            );
                            tracing::trace!("response body: {body}");
                            Ok(())
                        }
                        400..600 => Err(eyre!("http {status_code}: {body}")),
                        _ => Err(eyre!("unexpected response: http {status_code}: {body}")),
                    }
                }
                Err(e) => {
                    tracing::error!("{e}");
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
    retries: u16,
) -> Result<BatchResponse, ExporterError> {
    let response = response.map_err(|e| ExporterError::Fatal(e.into()))?;
    tracing::trace!("{:?}", &response);
    let status_code = response.status_code().as_u16();
    if status_code == 429 {
        return Err(ExporterError::RateLimited);
    }
    let body: Value = response
        .json()
        .await
        .map_err(|e| ExporterError::Fatal(e.into()))?;
    let mut items: Vec<Value> = body.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
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

    if (status_code != 200 && tracing::enabled!(tracing::Level::DEBUG))
        || (tracing::enabled!(tracing::Level::TRACE))
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
        )
        .map_err(ExporterError::Fatal)?;
    }

    match status_code {
        200 if error_count == 0 => tracing::debug!("{}, created {} docs", index, doc_count),
        200 => tracing::warn!(
            "{}, created {} docs with {} errors",
            index,
            doc_count,
            error_count
        ),
        400 => return Err(ExporterError::Fatal(eyre!("{} - http 400 bad request", index))),
        401 => return Err(ExporterError::Fatal(eyre!("{} - http 401 unauthorized", index))),
        403 => return Err(ExporterError::Fatal(eyre!("{} - http 403 forbidden", index))),
        404 => return Err(ExporterError::Fatal(eyre!("{} - http 404 not found", index))),
        413 => {
            return Err(ExporterError::Fatal(eyre!(
                "{} - http 413 request too large",
                index
            )));
        }
        500..=599 => {
            return Err(ExporterError::Fatal(eyre!(
                "{} - server errors: http {}",
                index,
                status_code
            )));
        }
        _ => tracing::warn!("unexpected http response: {}", status_code),
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
        )
        .map_err(ExporterError::Fatal)?;
    }

    let batch_response = BatchResponse {
        docs: item_count as u32,
        errors: error_count as u32,
        retries,
        size: 0,
        status_code,
        time: body.get("took").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
    };

    Ok(batch_response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, extract::State, http::StatusCode, response::IntoResponse, routing::post};
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    /// Spin up a minimal Axum server that returns `status_sequence` in order,
    /// falling back to 200 once the sequence is exhausted.
    async fn mock_bulk_server(status_sequence: Vec<u16>) -> (Url, Arc<Mutex<usize>>) {
        #[derive(Clone)]
        struct MockState {
            call_count: Arc<Mutex<usize>>,
            statuses: Arc<Vec<u16>>,
        }

        async fn bulk_handler(State(state): State<MockState>) -> impl IntoResponse {
            let idx = {
                let mut c = state.call_count.lock().unwrap();
                let v = *c;
                *c += 1;
                v
            };
            let status = state.statuses.get(idx).copied().unwrap_or(200);
            let body = match status {
                200 => json!({
                    "took": 1,
                    "errors": false,
                    "items": [{"create": {"_index": "test", "status": 201}}]
                }),
                429 => json!({
                    "status": 429,
                    "error": {
                        "type": "es_rejected_execution_exception",
                        "reason": "rejected execution of coordinating operation"
                    }
                }),
                _ => json!({
                    "status": status,
                    "error": {"type": "error", "reason": "error"}
                }),
            };
            (
                StatusCode::from_u16(status).unwrap(),
                axum::Json(body),
            )
        }

        let call_count = Arc::new(Mutex::new(0usize));
        let state = MockState {
            call_count: call_count.clone(),
            statuses: Arc::new(status_sequence),
        };

        let app = Router::new()
            .route("/{*path}", post(bulk_handler))
            .with_state(state);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let url = format!("http://{addr}").parse().unwrap();
        (url, call_count)
    }

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct RetryEnvGuard {
        prev_max: Option<String>,
        prev_initial: Option<String>,
        prev_max_ms: Option<String>,
    }

    impl RetryEnvGuard {
        fn set(max_retries: &str) -> Self {
            let prev_max = std::env::var("ESDIAG_EXPORT_RETRY_MAX").ok();
            let prev_initial = std::env::var("ESDIAG_EXPORT_RETRY_INITIAL_MS").ok();
            let prev_max_ms = std::env::var("ESDIAG_EXPORT_RETRY_MAX_MS").ok();
            unsafe {
                std::env::set_var("ESDIAG_EXPORT_RETRY_MAX", max_retries);
                std::env::set_var("ESDIAG_EXPORT_RETRY_INITIAL_MS", "1");
                std::env::set_var("ESDIAG_EXPORT_RETRY_MAX_MS", "5");
            }
            Self { prev_max, prev_initial, prev_max_ms }
        }
    }

    impl Drop for RetryEnvGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.prev_max {
                    Some(v) => std::env::set_var("ESDIAG_EXPORT_RETRY_MAX", v),
                    None => std::env::remove_var("ESDIAG_EXPORT_RETRY_MAX"),
                }
                match &self.prev_initial {
                    Some(v) => std::env::set_var("ESDIAG_EXPORT_RETRY_INITIAL_MS", v),
                    None => std::env::remove_var("ESDIAG_EXPORT_RETRY_INITIAL_MS"),
                }
                match &self.prev_max_ms {
                    Some(v) => std::env::set_var("ESDIAG_EXPORT_RETRY_MAX_MS", v),
                    None => std::env::remove_var("ESDIAG_EXPORT_RETRY_MAX_MS"),
                }
            }
        }
    }

    #[tokio::test]
    async fn retries_on_429_then_succeeds() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _env = RetryEnvGuard::set("5");
        let (url, call_count) = mock_bulk_server(vec![429, 429, 200]).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let result = exporter
            .batch_send("test-index".to_string(), vec![json!({"x": 1})])
            .await;

        assert!(result.is_ok(), "expected Ok");
        let br = result.unwrap();
        assert_eq!(br.retries, 2, "expected 2 retries");
        assert_eq!(br.errors, 0);
        assert_eq!(*call_count.lock().unwrap(), 3, "expected 3 total attempts");
    }

    #[tokio::test]
    async fn exhausted_retries_returns_ok_with_errors() {
        let _lock = ENV_LOCK.lock().unwrap();
        // max_retries=2 → 1 initial + 2 retries = 3 total, all 429
        let _env = RetryEnvGuard::set("2");
        let (url, call_count) = mock_bulk_server(vec![429, 429, 429]).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let docs = vec![json!({"a": 1}), json!({"b": 2})];
        let result = exporter
            .batch_send("test-index".to_string(), docs)
            .await;

        assert!(result.is_ok(), "exhaustion should return Ok, not Err");
        let br = result.unwrap();
        assert_eq!(br.errors, 2, "all docs should be counted as errors");
        assert_eq!(br.retries, 2);
        assert_eq!(br.status_code, 429);
        assert_eq!(*call_count.lock().unwrap(), 3, "expected 3 total attempts");
    }

    #[tokio::test]
    async fn fatal_status_not_retried() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _env = RetryEnvGuard::set("5");
        let (url, call_count) = mock_bulk_server(vec![400]).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let result = exporter
            .batch_send("test-index".to_string(), vec![json!({"x": 1})])
            .await;

        assert!(result.is_err(), "expected Err for 400");
        assert_eq!(*call_count.lock().unwrap(), 1, "400 must not be retried");
    }
}
