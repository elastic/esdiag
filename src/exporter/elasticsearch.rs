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
    kibana_base_url: Option<String>,
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
            kibana_base_url: super::kibana_base_url_from_env(),
            url,
            docs_tx: None,
            requires_secret,
        })
    }

    pub fn requires_secret(&self) -> bool {
        self.requires_secret
    }

    pub fn kibana_base_url(&self) -> Option<String> {
        self.kibana_base_url.clone()
    }

    /// Request to an arbitrary path on the Elasticsearch client
    pub async fn request(&self, method: &str, path: &str, value: Option<&Value>) -> Result<Response> {
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

    async fn send_value_batch(&self, index: String, values: Vec<Arc<Value>>) -> Result<BatchResponse> {
        let config = RetryConfig::from_env();
        let mut retries: u16 = 0;

        for attempt in 0..=config.max_retries {
            let batch: Vec<BulkOperation<Arc<Value>>> = values
                .iter()
                .map(|doc| BulkOperation::create(Arc::clone(doc)).pipeline("esdiag").into())
                .collect();

            let response = timeout(
                Self::request_timeout(),
                self.client.bulk(BulkParts::Index(&index)).body(batch).send(),
            )
            .await
            .map_err(|_| eyre!("Timed out sending bulk request to {} for index {}", self.url, index))?;

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
                    let mut response = BatchResponse::failed(values.len() as u32, 429);
                    response.retries = retries;
                    return Ok(response);
                }
                Err(ExporterError::Fatal(e)) => return Err(e),
            }
        }

        unreachable!("retry loop always returns within attempt bounds")
    }
}

impl TryFrom<KnownHost> for ElasticsearchExporter {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        let kibana_base_url = super::saved_viewer_kibana_base_url(&host).or_else(super::kibana_base_url_from_env);
        let requires_secret = !matches!(host.get_auth()?, Auth::None);
        let url = host.get_url()?;
        let client = ElasticsearchClient::try_from(host)?;
        let limit = std::env::var("ESDIAG_OUTPUT_TASK_LIMIT")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(10);

        tracing::debug!("Elasticsearch output task limit: {}", limit);

        Ok(Self {
            client,
            tx_limit: Arc::new(Semaphore::new(limit)),
            kibana_base_url,
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

    /// Sends documents to Elasticsearch, splitting into bulk sub-batches as needed
    /// and retrying each sub-batch on HTTP 429 with exponential backoff.
    /// Serialises docs to Values upfront so sub-batches can be resent without
    /// requiring `T: Clone`.
    async fn batch_send<T>(&self, index: String, docs: Vec<T>) -> Result<BatchResponse>
    where
        T: Serialize + Sized + Send + Sync,
    {
        let values: Vec<Arc<Value>> = docs
            .into_iter()
            .map(|doc| serde_json::to_value(doc).map(Arc::new))
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| eyre!("Failed to serialize document: {e}"))?;

        if values.is_empty() {
            let mut response = BatchResponse::aggregate();
            response.status_code = 200;
            return Ok(response);
        }

        let bulk_size = crate::env::get_int("ESDIAG_ES_BULK_SIZE")
            .unwrap_or(crate::env::ESDIAG_ES_BULK_SIZE)
            .max(1);
        let bulk_bytes = elasticsearch_bulk_bytes_limit();
        let batch_capacity = bulk_size.min(values.len());
        let mut batches = Vec::new();
        let mut batch = Vec::with_capacity(batch_capacity);
        let mut batch_bytes = 0usize;

        for value in values {
            let doc_bytes = bulk_bytes
                .map(|_| estimated_bulk_value_bytes(&index, value.as_ref()))
                .unwrap_or(0);
            let would_exceed_count = batch.len() >= bulk_size;
            let would_exceed_bytes = bulk_bytes
                .is_some_and(|max_bytes| !batch.is_empty() && batch_bytes.saturating_add(doc_bytes) > max_bytes);

            if would_exceed_count || would_exceed_bytes {
                batches.push(std::mem::take(&mut batch));
                batch = Vec::with_capacity(batch_capacity);
                batch_bytes = 0;
            }

            batch_bytes = batch_bytes.saturating_add(doc_bytes);
            batch.push(value);
        }

        if !batch.is_empty() {
            batches.push(batch);
        }

        let mut summary = BatchResponse::aggregate();
        for batch_index in 0..batches.len() {
            let batch_doc_count = batches[batch_index].len() as u32;
            let batch = std::mem::take(&mut batches[batch_index]);
            match self.send_value_batch(index.clone(), batch).await {
                Ok(response) => summary.merge(response),
                Err(err) if summary.batch_count == 0 => return Err(err),
                Err(err) => {
                    let unsent_docs = batches
                        .iter()
                        .skip(batch_index + 1)
                        .fold(0u32, |count, batch| count.saturating_add(batch.len() as u32));
                    let failed_docs = batch_doc_count.saturating_add(unsent_docs);
                    tracing::warn!(
                        "{index} - bulk sub-batch failed after partial success; recording {failed_docs} failed docs: {err}"
                    );
                    summary.merge(BatchResponse::failed(failed_docs, 0));
                    return Ok(summary);
                }
            }
        }

        Ok(summary)
    }

    /// Transmits a single batch of documents with semaphore-based connection limiting.
    /// Returns a one-shot channel for the BatchResponse.
    async fn batch_tx<T>(&self, index: String, docs: Vec<T>) -> Result<oneshot::Receiver<BatchResponse>>
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
            let _permit = semaphore.acquire().await.expect("Failed to acquire semaphore permit");

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
                    if tx.send(BatchResponse::failed(doc_count as u32, 0)).is_err() {
                        tracing::error!("Failed to send failed batch response: receiver dropped");
                    }
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
                            tracing::info!("metrics-diagnostic-esdiag, created diagnostic report {}", diagnostic_id);
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

fn elasticsearch_bulk_bytes_limit() -> Option<usize> {
    let bytes = crate::env::get_int("ESDIAG_ES_BULK_BYTES").unwrap_or(crate::env::ESDIAG_ES_BULK_BYTES);
    (bytes > 0).then_some(bytes)
}

fn estimated_bulk_value_bytes(index: &str, value: &Value) -> usize {
    const BULK_ACTION_OVERHEAD_BYTES: usize = 128;
    estimated_json_value_bytes(value)
        .saturating_add(index.len())
        .saturating_add(BULK_ACTION_OVERHEAD_BYTES)
}

fn estimated_json_value_bytes(value: &Value) -> usize {
    match value {
        Value::Null => 4,
        Value::Bool(true) => 4,
        Value::Bool(false) => 5,
        Value::Number(number) => number.to_string().len(),
        Value::String(value) => estimated_json_string_bytes(value),
        Value::Array(values) => values.iter().enumerate().fold(2usize, |bytes, (index, value)| {
            bytes
                .saturating_add(usize::from(index > 0))
                .saturating_add(estimated_json_value_bytes(value))
        }),
        Value::Object(values) => values.iter().enumerate().fold(2usize, |bytes, (index, (key, value))| {
            bytes
                .saturating_add(usize::from(index > 0))
                .saturating_add(estimated_json_string_bytes(key))
                .saturating_add(1)
                .saturating_add(estimated_json_value_bytes(value))
        }),
    }
}

fn estimated_json_string_bytes(value: &str) -> usize {
    value.bytes().fold(2usize, |bytes, byte| {
        bytes.saturating_add(match byte {
            b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 0x08 | 0x0c => 2,
            0x00..=0x1f => 6,
            _ => 1,
        })
    })
}

async fn parse_response(
    index: String,
    response: Result<Response, elasticsearch::Error>,
    retries: u16,
) -> Result<BatchResponse, ExporterError> {
    let response = response.map_err(|e| ExporterError::Fatal(e.into()))?;
    tracing::trace!("{:?}", &response);
    let status_code = response.status_code().as_u16();
    match status_code {
        200 => {}
        400 => {
            return Err(ExporterError::Fatal(eyre!("{} - http 400 bad request", index)));
        }
        401 => {
            return Err(ExporterError::Fatal(eyre!("{} - http 401 unauthorized", index)));
        }
        403 => {
            return Err(ExporterError::Fatal(eyre!("{} - http 403 forbidden", index)));
        }
        404 => {
            return Err(ExporterError::Fatal(eyre!("{} - http 404 not found", index)));
        }
        413 => {
            return Err(ExporterError::Fatal(eyre!("{} - http 413 request too large", index)));
        }
        429 => return Err(ExporterError::RateLimited),
        500..=599 => {
            return Err(ExporterError::Fatal(eyre!(
                "{} - server errors: http {}",
                index,
                status_code
            )));
        }
        _ => tracing::warn!("unexpected http response: {}", status_code),
    }
    let body: Value = response.json().await.map_err(|e| ExporterError::Fatal(e.into()))?;
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

    if (status_code != 200 && tracing::enabled!(tracing::Level::DEBUG)) || (tracing::enabled!(tracing::Level::TRACE)) {
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

    match error_count {
        0 => tracing::debug!("{}, created {} docs", index, doc_count),
        _ => tracing::warn!("{}, created {} docs with {} errors", index, doc_count, error_count),
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

    let mut batch_response = BatchResponse::new(doc_count as u32);
    batch_response.errors = error_count as u32;
    batch_response.retries = retries;
    batch_response.status_code = status_code;
    batch_response.time = body.get("took").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

    Ok(batch_response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, extract::State, http::StatusCode, response::IntoResponse, routing::post};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

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
                let mut c = state.call_count.lock().await;
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
            (StatusCode::from_u16(status).unwrap(), axum::Json(body))
        }

        let call_count = Arc::new(Mutex::new(0usize));
        let state = MockState {
            call_count: call_count.clone(),
            statuses: Arc::new(status_sequence),
        };

        let app = Router::new().route("/{*path}", post(bulk_handler)).with_state(state);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let url = format!("http://{addr}").parse().unwrap();
        (url, call_count)
    }

    async fn mock_bulk_server_raw(status: u16, body: &'static str) -> Url {
        #[derive(Clone)]
        struct MockState {
            status: u16,
            body: &'static str,
        }

        async fn bulk_handler(State(state): State<MockState>) -> impl IntoResponse {
            (StatusCode::from_u16(state.status).unwrap(), state.body)
        }

        let app = Router::new()
            .route("/{*path}", post(bulk_handler))
            .with_state(MockState { status, body });

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        format!("http://{addr}").parse().unwrap()
    }

    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

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
            Self {
                prev_max,
                prev_initial,
                prev_max_ms,
            }
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

    struct BulkBytesEnvGuard {
        prev_bytes: Option<String>,
    }

    impl BulkBytesEnvGuard {
        fn set(bytes: &str) -> Self {
            let prev_bytes = std::env::var("ESDIAG_ES_BULK_BYTES").ok();
            unsafe {
                std::env::set_var("ESDIAG_ES_BULK_BYTES", bytes);
            }
            Self { prev_bytes }
        }
    }

    impl Drop for BulkBytesEnvGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.prev_bytes {
                    Some(v) => std::env::set_var("ESDIAG_ES_BULK_BYTES", v),
                    None => std::env::remove_var("ESDIAG_ES_BULK_BYTES"),
                }
            }
        }
    }

    #[test]
    fn estimated_json_value_bytes_matches_serialized_value_length() {
        let value = json!({
            "array": [null, true, false, 123, "quoted \" text", "line\nbreak"],
            "nested": {
                "plain": "operate ascii",
                "control": "\u{001f}"
            }
        });

        let serialized = serde_json::to_vec(&value).expect("serialize value");

        assert_eq!(estimated_json_value_bytes(&value), serialized.len());
    }

    #[tokio::test]
    async fn retries_on_429_then_succeeds() {
        let _lock = ENV_LOCK.lock().await;
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
        assert_eq!(*call_count.lock().await, 3, "expected 3 total attempts");
    }

    #[tokio::test]
    async fn exhausted_retries_returns_ok_with_errors() {
        let _lock = ENV_LOCK.lock().await;
        // max_retries=2 → 1 initial + 2 retries = 3 total, all 429
        let _env = RetryEnvGuard::set("2");
        let (url, call_count) = mock_bulk_server(vec![429, 429, 429]).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let docs = vec![json!({"a": 1}), json!({"b": 2})];
        let result = exporter.batch_send("test-index".to_string(), docs).await;

        assert!(result.is_ok(), "exhaustion should return Ok, not Err");
        let br = result.unwrap();
        assert_eq!(br.errors, 2, "all docs should be counted as errors");
        assert_eq!(br.retries, 2);
        assert_eq!(br.status_code, 429);
        assert_eq!(*call_count.lock().await, 3, "expected 3 total attempts");
    }

    #[tokio::test]
    async fn fatal_status_not_retried() {
        let _lock = ENV_LOCK.lock().await;
        let _env = RetryEnvGuard::set("5");
        let (url, call_count) = mock_bulk_server(vec![400]).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let result = exporter
            .batch_send("test-index".to_string(), vec![json!({"x": 1})])
            .await;

        assert!(result.is_err(), "expected Err for 400");
        assert_eq!(*call_count.lock().await, 1, "400 must not be retried");
    }

    #[tokio::test]
    async fn request_too_large_reports_status_without_decoding_body() {
        let url = mock_bulk_server_raw(413, "request entity too large").await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let result = exporter
            .batch_send("test-index".to_string(), vec![json!({"x": 1})])
            .await;

        let err = match result {
            Ok(_) => panic!("expected 413 to fail"),
            Err(err) => err,
        };
        assert!(
            err.to_string().contains("test-index - http 413 request too large"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn split_send_preserves_successful_counts_after_later_fatal_error() {
        let _lock = ENV_LOCK.lock().await;
        let _env = BulkBytesEnvGuard::set("1");
        let (url, call_count) = mock_bulk_server(vec![200, 413]).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let result = exporter
            .batch_send(
                "test-index".to_string(),
                vec![json!({"x": "a"}), json!({"x": "b"}), json!({"x": "c"})],
            )
            .await
            .expect("partial success should return an aggregate response");

        assert_eq!(result.docs, 1);
        assert_eq!(result.errors, 2);
        assert_eq!(result.batch_count, 2);
        assert_eq!(result.status_code, 0);
        assert_eq!(
            *call_count.lock().await,
            2,
            "remaining docs should not be sent after fatal error"
        );
    }

    #[tokio::test]
    async fn batch_tx_reports_failed_batch_response() {
        let url = mock_bulk_server_raw(413, "request entity too large").await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let rx = exporter
            .batch_tx("test-index".to_string(), vec![json!({"x": 1}), json!({"x": 2})])
            .await
            .expect("batch tx starts");
        let response = rx.await.expect("failed batch response");

        assert_eq!(response.docs, 0);
        assert_eq!(response.errors, 2);
        assert_eq!(response.status_code, 0);
    }

    #[tokio::test]
    async fn exporter_send_splits_bulk_requests_by_configured_size() {
        let _lock = ENV_LOCK.lock().await;
        let _env = BulkBytesEnvGuard::set("104857600");
        let (url, call_count) = mock_bulk_server(vec![200, 200]).await;
        let exporter =
            crate::exporter::Exporter::Elasticsearch(ElasticsearchExporter::try_new(url, Auth::None).unwrap());
        let docs = (0..=crate::env::ESDIAG_ES_BULK_SIZE)
            .map(|id| json!({ "x": id }))
            .collect::<Vec<_>>();

        let result = exporter.send("test-index".to_string(), docs).await;

        assert!(result.is_ok(), "expected split send to succeed");
        assert_eq!(result.unwrap().batch_count, 2);
        assert_eq!(*call_count.lock().await, 2, "expected two bulk requests");
    }

    #[tokio::test]
    async fn exporter_send_splits_bulk_requests_by_estimated_bytes() {
        let _lock = ENV_LOCK.lock().await;
        let _env = BulkBytesEnvGuard::set("1");
        let (url, call_count) = mock_bulk_server(vec![200, 200, 200]).await;
        let exporter =
            crate::exporter::Exporter::Elasticsearch(ElasticsearchExporter::try_new(url, Auth::None).unwrap());
        let docs = vec![json!({"x": "a"}), json!({"x": "b"}), json!({"x": "c"})];

        let result = exporter.send("test-index".to_string(), docs).await;

        assert!(result.is_ok(), "expected byte split send to succeed");
        assert_eq!(result.unwrap().batch_count, 3);
        assert_eq!(*call_count.lock().await, 3, "expected three bulk requests");
    }

    #[tokio::test]
    async fn exporter_send_allows_disabling_byte_splitting() {
        let _lock = ENV_LOCK.lock().await;
        let _env = BulkBytesEnvGuard::set("0");
        let (url, call_count) = mock_bulk_server(vec![200]).await;
        let exporter =
            crate::exporter::Exporter::Elasticsearch(ElasticsearchExporter::try_new(url, Auth::None).unwrap());
        let docs = vec![json!({"x": "a"}), json!({"x": "b"}), json!({"x": "c"})];

        let result = exporter.send("test-index".to_string(), docs).await;

        assert!(result.is_ok(), "expected byte-disabled send to succeed");
        assert_eq!(result.unwrap().batch_count, 1);
        assert_eq!(*call_count.lock().await, 1, "expected one bulk request");
    }

    #[tokio::test]
    async fn exporter_send_reports_single_batch_send_failure() {
        let _lock = ENV_LOCK.lock().await;
        let _env = BulkBytesEnvGuard::set("0");
        let (url, call_count) = mock_bulk_server(vec![413]).await;
        let exporter =
            crate::exporter::Exporter::Elasticsearch(ElasticsearchExporter::try_new(url, Auth::None).unwrap());
        let docs = vec![json!({"x": "a"}), json!({"x": "b"})];

        let response = exporter
            .send("test-index".to_string(), docs)
            .await
            .expect("send failures should be recorded in the batch response");

        assert_eq!(response.docs, 0);
        assert_eq!(response.errors, 2);
        assert_eq!(response.batch_count, 1);
        assert_eq!(response.status_code, 0);
        assert_eq!(*call_count.lock().await, 1, "expected one bulk request");
    }
}
