// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::Export;
use crate::{
    client::{ElasticsearchBuilder, ElasticsearchClient, elasticsearch_client_from_output_host},
    data::{self, Auth, CredentialDirection, KnownHost},
    job::context::ExecutionScope,
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
    /// A failure that is not retried, with the HTTP status when the cluster rejected the request.
    Fatal {
        status: Option<u16>,
        error: eyre::Report,
    },
}

impl ExporterError {
    /// The cluster answered the request with an error status.
    fn rejected(status: u16, error: eyre::Report) -> Self {
        Self::Fatal {
            status: Some(status),
            error,
        }
    }

    /// The request or its handling failed without an error status from the cluster.
    fn incomplete(error: impl Into<eyre::Report>) -> Self {
        Self::Fatal {
            status: None,
            error: error.into(),
        }
    }
}

impl std::fmt::Display for ExporterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExporterError::RateLimited => write!(f, "http 429 too many requests"),
            ExporterError::Fatal { error, .. } => write!(f, "{error}"),
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
    pub(super) kibana_viewer: Option<Box<crate::data::Uri>>,
    url: Url,
    /// The execution this exporter sends for; `None` outside an execution.
    scope: Option<ExecutionScope>,
}

/// The export was stopped because its execution was cancelled.
fn cancelled_export() -> eyre::Report {
    eyre!("export cancelled")
}

impl ElasticsearchExporter {
    pub(super) fn with_scope(mut self, scope: ExecutionScope) -> Self {
        self.scope = Some(scope);
        self
    }

    fn is_cancelled(&self) -> bool {
        self.scope.as_ref().is_some_and(|scope| scope.token().is_cancelled())
    }

    /// Waits out a retry backoff. Returns `false`, at once, if the execution
    /// is or becomes cancelled, so no further request is sent.
    async fn backoff(&self, sleep_ms: u64) -> bool {
        let sleep = tokio::time::sleep(Duration::from_millis(sleep_ms));
        match &self.scope {
            Some(scope) => tokio::select! {
                biased;
                _ = scope.token().cancelled() => false,
                _ = sleep => true,
            },
            None => {
                sleep.await;
                true
            }
        }
    }
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
            kibana_viewer: crate::data::Uri::try_from_kibana_env().ok().map(Box::new),
            url,
            docs_tx: None,
            requires_secret,
            scope: None,
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

    /// Sends one sub-batch. The error next to the response is a fatal failure of a resend: the documents
    /// answered for before it still count, but nothing more should be sent.
    async fn send_value_batch(
        &self,
        index: String,
        values: Vec<Arc<Value>>,
    ) -> Result<(BatchResponse, Option<eyre::Report>)> {
        let config = RetryConfig::from_env();
        // The documents still to send: the whole batch, then only those Elasticsearch throttled per item.
        let mut pending = values;
        // What the documents already answered for came to, across resends of the throttled ones.
        let mut answered: Option<BatchResponse> = None;

        for attempt in 0..=config.max_retries {
            let retries = attempt;
            let can_retry = attempt < config.max_retries;
            let batch: Vec<BulkOperation<Arc<Value>>> = pending
                .iter()
                .map(|doc| BulkOperation::create(Arc::clone(doc)).pipeline("esdiag").into())
                .collect();

            let response = timeout(
                Self::request_timeout(),
                self.client.bulk(BulkParts::Index(&index)).body(batch).send(),
            )
            .await
            .map_err(|_| eyre!("Timed out sending bulk request to {} for index {}", self.url, index));
            let response = match response {
                Ok(response) => response,
                Err(e) => return finish_after_failure(&index, answered, pending.len(), None, e, retries),
            };

            let sleep_ms = match parse_response(index.clone(), response, retries, can_retry).await {
                Ok((batch_response, throttled)) => {
                    match answered.as_mut() {
                        Some(answered) => answered.merge_resend(batch_response),
                        None => answered = Some(batch_response),
                    }
                    if throttled.is_empty() {
                        let mut response = answered.expect("a response was just recorded");
                        response.retries = retries;
                        return Ok((response, None));
                    }
                    let sleep_ms = backoff_ms(attempt, &config);
                    tracing::warn!(
                        "{index} - {} of {} docs rejected with http 429, retry {}/{}, sleeping {sleep_ms}ms",
                        throttled.len(),
                        pending.len(),
                        u32::from(attempt) + 1,
                        config.max_retries,
                    );
                    pending = throttled
                        .iter()
                        .filter_map(|&item| pending.get(item).cloned())
                        .collect();
                    sleep_ms
                }
                Err(ExporterError::RateLimited) if can_retry => {
                    let sleep_ms = backoff_ms(attempt, &config);
                    tracing::warn!(
                        "{index} - http 429, retry {}/{}, sleeping {sleep_ms}ms",
                        u32::from(attempt) + 1,
                        config.max_retries,
                    );
                    sleep_ms
                }
                Err(ExporterError::RateLimited) => {
                    tracing::error!(
                        "{index} - http 429, {} docs dropped after {} attempts",
                        pending.len(),
                        u32::from(config.max_retries) + 1,
                    );
                    let mut response = match answered {
                        Some(mut answered) => {
                            answered.merge_resend(BatchResponse::failed(pending.len() as u32, 429));
                            answered.status_code = 429;
                            answered
                        }
                        None => BatchResponse::failed(pending.len() as u32, 429),
                    };
                    response.retries = retries;
                    return Ok((response, None));
                }
                Err(ExporterError::Fatal { status, error }) => {
                    return finish_after_failure(&index, answered, pending.len(), status, error, retries);
                }
            };
            if !self.backoff(sleep_ms).await {
                return finish_after_failure(&index, answered, pending.len(), None, cancelled_export(), retries);
            }
        }

        unreachable!("retry loop always returns within attempt bounds")
    }

    /// Splits serialized documents into bulk sub-batches and sends them in turn,
    /// starting none once the execution is cancelled.
    async fn send_values(&self, index: String, values: Vec<Arc<Value>>) -> Result<BatchResponse> {
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
            if self.is_cancelled() {
                if summary.batch_count == 0 {
                    return Err(cancelled_export());
                }
                let unsent_docs = doc_count(&batches[batch_index..]);
                tracing::warn!("{index} - export cancelled; recording {unsent_docs} unsent docs as failed");
                summary.merge(BatchResponse::failed(
                    unsent_docs,
                    BatchResponse::HTTP_REQUEST_NOT_COMPLETED,
                ));
                return Ok(summary);
            }
            let batch_doc_count = batches[batch_index].len() as u32;
            let batch = std::mem::take(&mut batches[batch_index]);
            match self.send_value_batch(index.clone(), batch).await {
                Ok((response, None)) => summary.merge(response),
                Ok((response, Some(err))) => {
                    summary.merge(response);
                    let unsent_docs = doc_count(&batches[batch_index + 1..]);
                    if unsent_docs > 0 {
                        tracing::warn!(
                            "{index} - stopping after a failed resend; recording {unsent_docs} unsent docs as failed: {err}"
                        );
                        summary.merge(BatchResponse::failed(
                            unsent_docs,
                            BatchResponse::HTTP_REQUEST_NOT_COMPLETED,
                        ));
                    }
                    return Ok(summary);
                }
                Err(err) if summary.batch_count == 0 => return Err(err),
                Err(err) => {
                    let unsent_docs = doc_count(&batches[batch_index + 1..]);
                    let failed_docs = batch_doc_count.saturating_add(unsent_docs);
                    tracing::warn!(
                        "{index} - bulk sub-batch failed after partial success; recording {failed_docs} failed docs: {err}"
                    );
                    summary.merge(BatchResponse::failed(
                        failed_docs,
                        BatchResponse::HTTP_REQUEST_NOT_COMPLETED,
                    ));
                    return Ok(summary);
                }
            }
        }

        Ok(summary)
    }
}

impl TryFrom<KnownHost> for ElasticsearchExporter {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        let viewer = super::output_viewer(&host);
        let kibana_base_url = viewer.as_ref().and_then(super::kibana_base_url_for_output);
        let kibana_viewer = kibana_base_url.as_ref().and(viewer).map(crate::data::Uri::KnownHost);
        let requires_secret = !matches!(host.get_auth_for_direction(CredentialDirection::Output)?, Auth::None);
        let url = host.get_url()?;
        let client = elasticsearch_client_from_output_host(host)?;
        let limit = std::env::var("ESDIAG_OUTPUT_TASK_LIMIT")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(10);

        tracing::debug!("Elasticsearch output task limit: {}", limit);

        Ok(Self {
            client,
            tx_limit: Arc::new(Semaphore::new(limit)),
            kibana_base_url,
            kibana_viewer: kibana_viewer.map(Box::new),
            url,
            docs_tx: None,
            requires_secret,
            scope: None,
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

        // A tracked request outlives a dropped caller, so the scope ends only once it has.
        match self.scope.clone() {
            Some(scope) => {
                let exporter = self.clone();
                scope
                    .tracker()
                    .spawn(async move { exporter.send_values(index, values).await })
                    .await
                    .map_err(|e| eyre!("Bulk export task failed: {e}"))?
            }
            None => self.send_values(index, values).await,
        }
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
        let scope = self.scope.clone();

        let token = scope.as_ref().map(|scope| scope.token().clone());
        let task = async move {
            // Acquire semaphore permit inside task - blocks if at limit (backpressure)
            let acquire = semaphore.acquire();
            let _permit = match &token {
                // Other executions sharing this exporter may hold every permit.
                Some(token) => tokio::select! {
                    biased;
                    _ = token.cancelled() => return,
                    permit = acquire => permit,
                },
                None => acquire.await,
            }
            .expect("Failed to acquire semaphore permit");
            // A dropped receiver means the export was abandoned, such as a cancelled request.
            if tx.is_closed() {
                return;
            }

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
                    if tx
                        .send(BatchResponse::failed(
                            doc_count as u32,
                            BatchResponse::HTTP_REQUEST_NOT_COMPLETED,
                        ))
                        .is_err()
                    {
                        tracing::error!("Failed to send failed batch response: receiver dropped");
                    }
                }
            }
        };
        match scope {
            Some(scope) => scope.tracker().spawn(task),
            None => tokio::spawn(task),
        };

        Ok(rx)
    }

    /// Sends the final diagnostic report document to Elasticsearch.
    async fn save_report(&self, report: &DiagnosticReport) -> Result<()> {
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
                    if matches!(body.get("failure_store").and_then(Value::as_str), Some("used")) {
                        let destination = body
                            .get("_index")
                            .and_then(Value::as_str)
                            .filter(|value| !value.is_empty())
                            .map(rejected_data_stream)
                            .unwrap_or("metrics-diagnostic-esdiag");
                        return Err(eyre!(
                            "Diagnostic report {} was retained in {}::failures; inspect error.message for the rejection reason: {}",
                            diagnostic_id,
                            destination,
                            body
                        ));
                    }
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

// Elasticsearch bulk items identify backing indices after ingest rerouting.
// Only recognize ESDiag's stream names; preserve other index names verbatim.
fn rejected_data_stream(index: &str) -> &str {
    let Some(backing) = index.strip_prefix(".ds-").or_else(|| index.strip_prefix(".fs-")) else {
        return index;
    };
    let Some((dated_stream, generation)) = backing.rsplit_once('-') else {
        return index;
    };
    let Some((stream, date)) = dated_stream.rsplit_once('-') else {
        return index;
    };
    if generation.len() < 6
        || !generation.bytes().all(|byte| byte.is_ascii_digit())
        || chrono::NaiveDate::parse_from_str(date, "%Y.%m.%d").is_err()
        || !(stream.ends_with("-esdiag") || stream.contains("-esdiag-"))
    {
        return index;
    }
    stream
}

fn doc_count(batches: &[Vec<Arc<Value>>]) -> u32 {
    batches
        .iter()
        .fold(0, |count, batch| count.saturating_add(batch.len() as u32))
}

/// A bulk request that failed outright. Before any document was answered for, the batch fails as before.
/// When a resend of throttled documents fails, the documents already created still count, and only the
/// unsent ones are recorded as failed, with the status the cluster answered, if it did.
fn finish_after_failure(
    index: &str,
    answered: Option<BatchResponse>,
    unsent: usize,
    status: Option<u16>,
    error: eyre::Report,
    retries: u16,
) -> Result<(BatchResponse, Option<eyre::Report>)> {
    let Some(mut response) = answered else {
        return Err(error);
    };
    tracing::warn!("{index} - resending {unsent} throttled docs failed; recording them as failed: {error}");
    let status_code = status.unwrap_or(BatchResponse::HTTP_REQUEST_NOT_COMPLETED);
    response.merge_resend(BatchResponse::failed(unsent as u32, status_code));
    response.status_code = status_code;
    response.retries = retries;
    Ok((response, Some(error)))
}

/// Reads a bulk response. With `retry_throttled`, items Elasticsearch rejected with `429` (its write queue
/// was full) are not counted: their positions in the request are returned so they can be sent again.
async fn parse_response(
    index: String,
    response: Result<Response, elasticsearch::Error>,
    retries: u16,
    retry_throttled: bool,
) -> Result<(BatchResponse, Vec<usize>), ExporterError> {
    let response = response.map_err(ExporterError::incomplete)?;
    tracing::trace!("{:?}", &response);
    let status_code = response.status_code().as_u16();
    match status_code {
        200 => {}
        400 => {
            return Err(ExporterError::rejected(
                status_code,
                eyre!("{} - http 400 bad request", index),
            ));
        }
        401 => {
            return Err(ExporterError::rejected(
                status_code,
                eyre!("{} - http 401 unauthorized", index),
            ));
        }
        403 => {
            return Err(ExporterError::rejected(
                status_code,
                eyre!("{} - http 403 forbidden", index),
            ));
        }
        404 => {
            return Err(ExporterError::rejected(
                status_code,
                eyre!("{} - http 404 not found", index),
            ));
        }
        413 => {
            return Err(ExporterError::rejected(
                status_code,
                eyre!("{} - http 413 request too large", index),
            ));
        }
        429 => return Err(ExporterError::RateLimited),
        500..=599 => {
            return Err(ExporterError::rejected(
                status_code,
                eyre!("{} - server errors: http {}", index, status_code),
            ));
        }
        _ => tracing::warn!("unexpected http response: {}", status_code),
    }
    let body: Value = response.json().await.map_err(ExporterError::incomplete)?;
    let items: Vec<Value> = body.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
    let item_count = items.len();

    let mut throttled = Vec::new();
    let mut error_items = Vec::new();
    for (position, item) in items.into_iter().enumerate() {
        let status = item["create"]["status"].as_u64();
        if retry_throttled && status == Some(429) {
            throttled.push(position);
        } else if status != Some(201) || item["create"]["failure_store"] == "used" {
            error_items.push(item);
        }
    }
    let error_count = error_items.len();
    let doc_count = item_count - error_count - throttled.len();

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
        .map_err(ExporterError::incomplete)?;
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
        .map_err(ExporterError::incomplete)?;
    }

    let mut batch_response = BatchResponse::new(doc_count as u32);
    for item in &error_items {
        let destination = item["create"]["_index"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or(&index);
        let destination = rejected_data_stream(destination);
        *batch_response
            .rejected_indices
            .entry(destination.to_string())
            .or_default() += 1;
        let error = &item["create"]["error"];
        let reason = match error.get("reason").and_then(Value::as_str) {
            Some(reason) => format!("{}: {reason}", error["type"].as_str().unwrap_or("document_rejected")),
            None if item["create"]["failure_store"] == "used" => {
                format!("Document retained in {destination}::failures; inspect error.message for the rejection reason")
            }
            None => format!("Bulk item rejected with status {}", item["create"]["status"]),
        };
        let reason: String = reason.chars().take(1024).collect();
        let samples = batch_response
            .rejection_reasons
            .entry(destination.to_string())
            .or_default();
        if samples.len() < 3 && !samples.contains(&reason) {
            samples.push(reason);
        }
    }
    batch_response.errors = error_count as u32;
    batch_response.retries = retries;
    batch_response.status_code = status_code;
    batch_response.time = body.get("took").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

    Ok((batch_response, throttled))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::es_mock::{BulkReply, DocReply, ElasticsearchMock};
    use serde_json::json;
    use tokio::sync::Mutex;

    /// Serve the shared Elasticsearch mock with scripted `_bulk` replies;
    /// requests beyond the script create every document.
    async fn mock_bulk_server(replies: Vec<BulkReply>) -> (Url, ElasticsearchMock) {
        let mock = ElasticsearchMock::new().bulk_replies(replies);
        let url = mock.serve().await.parse().unwrap();
        (url, mock)
    }

    async fn mock_doc_server(reply: DocReply) -> (Url, ElasticsearchMock) {
        let mock = ElasticsearchMock::new().doc_replies([reply]);
        let url = mock.serve().await.parse().unwrap();
        (url, mock)
    }

    fn test_report() -> DiagnosticReport {
        let metadata = crate::processor::diagnostic::DiagnosticMetadata {
            id: "report-save-test".to_string(),
            collection_date: 0,
            runner: "test".to_string(),
            version: None,
            uuid: "report-save-test".to_string(),
        };
        DiagnosticReport::try_from(
            crate::processor::diagnostic::DiagnosticReportBuilder::from(metadata)
                .receiver("file report-save-test.zip".to_string()),
        )
        .expect("diagnostic report")
    }

    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    #[test]
    fn rejected_backing_indices_preserve_the_complete_stream_name() {
        assert_eq!(
            rejected_data_stream(".ds-settings-index-esdiag-migrated-hist-2026.09.06-000013"),
            "settings-index-esdiag-migrated-hist"
        );
        assert_eq!(
            rejected_data_stream(".ds-health-impact-esdiag-not-a-backing-index"),
            ".ds-health-impact-esdiag-not-a-backing-index"
        );
        assert_eq!(
            rejected_data_stream(".ds-other-stream-2026.09.06-000001"),
            ".ds-other-stream-2026.09.06-000001"
        );
    }

    #[tokio::test]
    async fn rerouted_failures_reach_the_diagnostic_report() {
        // Bulk split settings are process-wide; other tests change them.
        let _lock = ENV_LOCK.lock().await;
        let mut env = crate::TestEnv::new();
        env.set_path("ESDIAG_HOME", env.tmp.path().to_path_buf());
        std::fs::create_dir_all(env.tmp.path().join("last_run")).unwrap();
        use crate::processor::{
            ProcessorSummary,
            diagnostic::{DiagnosticMetadata, DiagnosticReportBuilder},
        };
        let (url, mock) = mock_bulk_server(vec![BulkReply::Items(vec![
            json!({"_index":".ds-health-indicator-esdiag-2026.09.06-000001","_id":"a","_version":1,"result":"created","status":201}),
            json!({"_index":".ds-health-impact-esdiag-2026.09.06-000001","_id":"b","status":400,"error":{"type":"document_parsing_exception","reason":"Cannot write to a field alias [diagnostic.platform]"}}),
            json!({"_index":"health-diagnosis-esdiag","_id":"c","status":400,"error":{"type":"illegal_argument_exception","reason":"only write ops with an op_type of create are allowed in data streams"}}),
            json!({"_index":".fs-health-impact-esdiag-2026.09.06-000002","_id":"d","_version":1,"result":"created","status":201,"failure_store":"used"}),
        ])])
        .await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();
        let batch = exporter
            .batch_send("health-indicator-esdiag".into(), vec![json!({}); 4])
            .await
            .unwrap();
        assert_eq!(batch.docs, 1);
        assert_eq!(batch.errors, 3);
        assert_eq!(mock.bulk_requests()[0].documents.len(), 4);
        let mut summary = ProcessorSummary::new("health-indicator-esdiag".into());
        summary.add_batch(batch);
        // Include a request failure with no resolved destination.
        summary.add_batch(BatchResponse::failed(2, 429));
        let metadata = DiagnosticMetadata {
            id: "test".into(),
            collection_date: 0,
            runner: "test".into(),
            version: None,
            uuid: "test".into(),
        };
        let mut report =
            DiagnosticReport::try_from(DiagnosticReportBuilder::from(metadata).receiver("file fixture".into()))
                .unwrap();
        report.add_processor_summary(summary.was_parsed());
        assert_eq!(
            report.rejected_indices(),
            vec![
                ("health-diagnosis-esdiag".into(), 1),
                ("health-impact-esdiag".into(), 2),
                ("health-indicator-esdiag".into(), 2),
            ]
        );
        assert_eq!(report.diagnostic.docs.errors, 5);
        let rendered = serde_json::to_value(&report.diagnostic).unwrap()["events"].to_string();
        assert!(rendered.contains("Cannot write to a field alias [diagnostic.platform]"));
        assert!(rendered.contains("health-impact-esdiag::failures"));
    }

    #[tokio::test]
    async fn save_report_failure_store_capture_is_not_report_success() {
        let mut env = crate::TestEnv::new();
        env.set_path("ESDIAG_HOME", env.tmp.path().to_path_buf());
        let (url, mock) = mock_doc_server(DocReply::FailureStore).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let error = exporter
            .save_report(&test_report())
            .await
            .expect_err("failure-store capture must fail report persistence");
        let message = error.to_string();
        assert!(
            message.contains("metrics-diagnostic-esdiag::failures"),
            "missing recovery guidance: {message}"
        );
        assert!(
            message.contains("error.message"),
            "missing failure-store inspection guidance: {message}"
        );
        let requests = mock.doc_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].index, "metrics-diagnostic-esdiag");
        assert_eq!(requests[0].pipeline.as_deref(), Some("esdiag"));
    }

    #[tokio::test]
    async fn save_report_regular_http_and_transport_failures_are_errors() {
        let mut env = crate::TestEnv::new();
        env.set_path("ESDIAG_HOME", env.tmp.path().to_path_buf());
        let (url, _mock) = mock_doc_server(DocReply::Status(400)).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();
        let error = exporter
            .save_report(&test_report())
            .await
            .expect_err("HTTP report rejection must fail persistence");
        assert!(error.to_string().contains("http 400"), "unexpected HTTP error: {error}");

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let exporter =
            ElasticsearchExporter::try_new(format!("http://{address}").parse().unwrap(), Auth::None).unwrap();
        assert!(
            exporter.save_report(&test_report()).await.is_err(),
            "transport report failure must fail persistence"
        );
    }

    struct RetryEnvGuard {
        prev_max: Option<String>,
        prev_initial: Option<String>,
        prev_max_ms: Option<String>,
    }

    impl RetryEnvGuard {
        fn set(max_retries: &str) -> Self {
            Self::with_backoff(max_retries, "1", "5")
        }

        fn with_backoff(max_retries: &str, initial_ms: &str, max_ms: &str) -> Self {
            let prev_max = std::env::var("ESDIAG_EXPORT_RETRY_MAX").ok();
            let prev_initial = std::env::var("ESDIAG_EXPORT_RETRY_INITIAL_MS").ok();
            let prev_max_ms = std::env::var("ESDIAG_EXPORT_RETRY_MAX_MS").ok();
            unsafe {
                std::env::set_var("ESDIAG_EXPORT_RETRY_MAX", max_retries);
                std::env::set_var("ESDIAG_EXPORT_RETRY_INITIAL_MS", initial_ms);
                std::env::set_var("ESDIAG_EXPORT_RETRY_MAX_MS", max_ms);
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
    async fn bulk_requests_create_documents_through_the_esdiag_pipeline() {
        let _lock = ENV_LOCK.lock().await;
        let (url, mock) = mock_bulk_server(vec![]).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();
        let docs = vec![json!({"x": 1}), json!({"x": 2}), json!({"x": 3})];

        let response = exporter
            .batch_send("metrics-node-esdiag".to_string(), docs.clone())
            .await
            .expect("bulk send");

        assert_eq!(response.docs, 3);
        assert_eq!(response.errors, 0);
        let requests = mock.bulk_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path_index, "metrics-node-esdiag");
        assert_eq!(requests[0].documents, docs);
        for action in &requests[0].actions {
            assert_eq!(action.op, "create", "data streams only accept create");
            assert_eq!(action.index, "metrics-node-esdiag");
            assert_eq!(action.pipeline.as_deref(), Some("esdiag"));
        }
    }

    #[tokio::test]
    async fn retries_on_429_then_succeeds() {
        let _lock = ENV_LOCK.lock().await;
        let _env = RetryEnvGuard::set("5");
        let (url, mock) = mock_bulk_server(vec![BulkReply::Status(429), BulkReply::Status(429)]).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let br = exporter
            .batch_send("test-index".to_string(), vec![json!({"x": 1})])
            .await
            .expect("retried send succeeds");

        assert_eq!(br.retries, 2, "expected 2 retries");
        assert_eq!(br.docs, 1);
        assert_eq!(br.errors, 0);
        assert_eq!(mock.bulk_requests().len(), 3, "expected 3 total attempts");
    }

    #[tokio::test]
    async fn exhausted_retries_returns_ok_with_errors() {
        let _lock = ENV_LOCK.lock().await;
        // max_retries=2 → 1 initial + 2 retries = 3 total, all 429
        let _env = RetryEnvGuard::set("2");
        let (url, mock) = mock_bulk_server(vec![BulkReply::Status(429); 3]).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let docs = vec![json!({"a": 1}), json!({"b": 2})];
        let br = exporter
            .batch_send("test-index".to_string(), docs)
            .await
            .expect("exhaustion should return Ok, not Err");

        assert_eq!(br.docs, 0);
        assert_eq!(br.errors, 2, "all docs should be counted as errors");
        assert_eq!(br.retries, 2);
        assert_eq!(br.status_code, 429);
        assert_eq!(mock.bulk_requests().len(), 3, "expected 3 total attempts");
    }

    /// The item Elasticsearch answers when its write queue is full.
    fn throttled_item() -> Value {
        json!({"_index":".ds-test-esdiag-2026.11.21-000001","status":429,"error":{"type":"es_rejected_execution_exception","reason":"rejected execution of coordinating operation"}})
    }

    fn created_item(id: &str) -> Value {
        json!({"_index":".ds-test-esdiag-2026.11.21-000001","_id":id,"_version":1,"result":"created","status":201})
    }

    // https://github.com/elastic/esdiag/issues/463
    #[tokio::test]
    async fn retries_only_the_items_rejected_with_429() {
        let _lock = ENV_LOCK.lock().await;
        let _env = RetryEnvGuard::set("5");
        let mut test_env = crate::TestEnv::new();
        test_env.set_path("ESDIAG_HOME", test_env.tmp.path().to_path_buf());
        std::fs::create_dir_all(test_env.tmp.path().join("last_run")).unwrap();
        let first = BulkReply::Items(vec![
            created_item("a"),
            throttled_item(),
            json!({"_index":".ds-test-esdiag-2026.11.21-000001","status":400,"error":{"type":"document_parsing_exception","reason":"failed to parse field [x]"}}),
            throttled_item(),
        ]);
        // The resend of documents 2 and 4 is throttled again, for the second of them only.
        let second = BulkReply::Items(vec![created_item("b"), throttled_item()]);
        let (url, mock) = mock_bulk_server(vec![first, second]).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let docs = vec![json!({"n": 1}), json!({"n": 2}), json!({"n": 3}), json!({"n": 4})];
        let br = exporter.batch_send("test-esdiag".to_string(), docs).await.unwrap();

        // The throttled documents are created by the resends; the parsing error stays permanent.
        assert_eq!(br.docs, 3);
        assert_eq!(br.errors, 1);
        assert_eq!(br.retries, 2);
        assert_eq!(br.batch_count, 1);
        assert_eq!(br.status_code, 200);
        assert_eq!(br.rejected_indices.get("test-esdiag"), Some(&1));
        let requests = mock.bulk_requests();
        assert_eq!(requests.len(), 3, "expected the first request and two resends");
        assert_eq!(
            requests[1].documents,
            vec![json!({"n": 2}), json!({"n": 4})],
            "resend only the throttled documents, in order"
        );
        assert_eq!(
            requests[2].documents,
            vec![json!({"n": 4})],
            "positions are those of the previous resend, not of the batch"
        );
    }

    #[tokio::test]
    async fn items_still_rejected_with_429_after_retries_are_errors() {
        let _lock = ENV_LOCK.lock().await;
        // max_retries=1: the first request and one resend.
        let _env = RetryEnvGuard::set("1");
        let mut test_env = crate::TestEnv::new();
        test_env.set_path("ESDIAG_HOME", test_env.tmp.path().to_path_buf());
        std::fs::create_dir_all(test_env.tmp.path().join("last_run")).unwrap();
        let (url, mock) = mock_bulk_server(vec![
            BulkReply::Items(vec![created_item("a"), throttled_item()]),
            BulkReply::Items(vec![throttled_item()]),
        ])
        .await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let br = exporter
            .batch_send("test-esdiag".to_string(), vec![json!({"n": 1}), json!({"n": 2})])
            .await
            .unwrap();

        assert_eq!(br.docs, 1);
        assert_eq!(br.errors, 1);
        assert_eq!(br.retries, 1);
        assert_eq!(
            mock.bulk_requests().len(),
            2,
            "no resend beyond ESDIAG_EXPORT_RETRY_MAX"
        );
        assert_eq!(br.rejected_indices.get("test-esdiag"), Some(&1));
        assert_eq!(
            br.rejection_reasons.get("test-esdiag").map(Vec::as_slice),
            Some(
                ["es_rejected_execution_exception: rejected execution of coordinating operation".to_string()]
                    .as_slice()
            )
        );
    }

    #[tokio::test]
    async fn a_failed_resend_keeps_the_documents_already_created() {
        let _lock = ENV_LOCK.lock().await;
        let _env = RetryEnvGuard::set("5");
        let (url, mock) = mock_bulk_server(vec![
            BulkReply::Items(vec![created_item("a"), throttled_item()]),
            BulkReply::Status(400),
        ])
        .await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let br = exporter
            .batch_send("test-esdiag".to_string(), vec![json!({"n": 1}), json!({"n": 2})])
            .await
            .expect("a failed resend still reports the created document");

        assert_eq!(br.docs, 1);
        assert_eq!(br.errors, 1);
        assert_eq!(br.retries, 1);
        assert_eq!(
            br.status_code, 400,
            "the cluster's status, not the no-response sentinel"
        );
        assert_eq!(mock.bulk_requests().len(), 2);
    }

    #[tokio::test]
    async fn a_failed_resend_stops_the_remaining_sub_batches() {
        let _lock = ENV_LOCK.lock().await;
        let _env = RetryEnvGuard::set("5");
        let _bulk = BulkBytesEnvGuard::set("104857600");
        let sub_batch = crate::env::ESDIAG_ES_BULK_SIZE;
        let mut items = vec![created_item("a"); sub_batch - 1];
        items.push(throttled_item());
        let (url, mock) = mock_bulk_server(vec![BulkReply::Items(items), BulkReply::Status(401)]).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        // Two sub-batches: the second one, of 3 documents, is never sent.
        let docs = (0..sub_batch + 3).map(|n| json!({"n": n})).collect();
        let br = exporter.batch_send("test-esdiag".to_string(), docs).await.unwrap();

        assert_eq!(br.docs, sub_batch as u32 - 1);
        assert_eq!(br.errors, 1 + 3);
        assert_eq!(
            mock.bulk_requests().len(),
            2,
            "no sub-batch is sent after the failed resend"
        );
    }

    #[tokio::test]
    async fn a_resend_without_a_usable_response_is_not_completed() {
        let _lock = ENV_LOCK.lock().await;
        let _env = RetryEnvGuard::set("5");
        let (url, mock) = mock_bulk_server(vec![
            BulkReply::Items(vec![created_item("a"), throttled_item()]),
            BulkReply::Proxy(200, "<html>not Elasticsearch</html>"),
        ])
        .await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let br = exporter
            .batch_send("test-esdiag".to_string(), vec![json!({"n": 1}), json!({"n": 2})])
            .await
            .expect("a failed resend still reports the created document");

        assert_eq!(br.docs, 1);
        assert_eq!(br.errors, 1);
        assert_eq!(br.status_code, BatchResponse::HTTP_REQUEST_NOT_COMPLETED);
        assert_eq!(mock.bulk_requests().len(), 2);
    }

    #[tokio::test]
    async fn a_resend_rejected_with_request_429_keeps_the_documents_already_created() {
        let _lock = ENV_LOCK.lock().await;
        let _env = RetryEnvGuard::set("1");
        let (url, mock) = mock_bulk_server(vec![
            BulkReply::Items(vec![created_item("a"), throttled_item()]),
            BulkReply::Status(429),
        ])
        .await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let br = exporter
            .batch_send("test-esdiag".to_string(), vec![json!({"n": 1}), json!({"n": 2})])
            .await
            .unwrap();

        assert_eq!((br.docs, br.errors, br.status_code), (1, 1, 429));
        assert_eq!(mock.bulk_requests().len(), 2);
    }

    #[tokio::test]
    async fn fatal_status_not_retried() {
        let _lock = ENV_LOCK.lock().await;
        let _env = RetryEnvGuard::set("5");
        let (url, mock) = mock_bulk_server(vec![BulkReply::Status(400)]).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let result = exporter
            .batch_send("test-index".to_string(), vec![json!({"x": 1})])
            .await;

        assert!(result.is_err(), "expected Err for 400");
        assert_eq!(mock.bulk_requests().len(), 1, "400 must not be retried");
    }

    #[tokio::test]
    async fn request_too_large_reports_status_without_decoding_body() {
        // Proxies answer 413 with plain text; the status must be handled before any body decoding.
        let (url, _mock) = mock_bulk_server(vec![BulkReply::Proxy(413, "request entity too large")]).await;
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
        let (url, mock) = mock_bulk_server(vec![BulkReply::Created, BulkReply::Status(413)]).await;
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
        // HTTP request never completed: 599 sentinel, never 0 (ADR-0016)
        assert_eq!(result.status_code, BatchResponse::HTTP_REQUEST_NOT_COMPLETED);
        assert_eq!(
            mock.bulk_requests().len(),
            2,
            "remaining docs should not be sent after fatal error"
        );
    }

    #[tokio::test]
    async fn batch_tx_reports_failed_batch_response() {
        let (url, _mock) = mock_bulk_server(vec![BulkReply::Status(413)]).await;
        let exporter = ElasticsearchExporter::try_new(url, Auth::None).unwrap();

        let rx = exporter
            .batch_tx("test-index".to_string(), vec![json!({"x": 1}), json!({"x": 2})])
            .await
            .expect("batch tx starts");
        let response = rx.await.expect("failed batch response");

        assert_eq!(response.docs, 0);
        assert_eq!(response.errors, 2);
        assert_eq!(response.status_code, BatchResponse::HTTP_REQUEST_NOT_COMPLETED);
    }

    #[tokio::test]
    async fn exporter_send_splits_bulk_requests_by_configured_size() {
        let _lock = ENV_LOCK.lock().await;
        let _env = BulkBytesEnvGuard::set("104857600");
        let (url, mock) = mock_bulk_server(vec![]).await;
        let exporter =
            crate::exporter::Exporter::Elasticsearch(ElasticsearchExporter::try_new(url, Auth::None).unwrap());
        let docs = (0..=crate::env::ESDIAG_ES_BULK_SIZE)
            .map(|id| json!({ "x": id }))
            .collect::<Vec<_>>();
        let total = docs.len();

        let result = exporter.send("test-index".to_string(), docs).await.expect("split send");

        assert_eq!(result.batch_count, 2);
        assert_eq!(result.docs as usize, total);
        let requests = mock.bulk_requests();
        assert_eq!(requests.len(), 2, "expected two bulk requests");
        assert_eq!(requests[0].documents.len(), crate::env::ESDIAG_ES_BULK_SIZE);
        assert_eq!(requests[1].documents.len(), 1);
    }

    #[tokio::test]
    async fn exporter_send_splits_bulk_requests_by_estimated_bytes() {
        let _lock = ENV_LOCK.lock().await;
        let _env = BulkBytesEnvGuard::set("1");
        let (url, mock) = mock_bulk_server(vec![]).await;
        let exporter =
            crate::exporter::Exporter::Elasticsearch(ElasticsearchExporter::try_new(url, Auth::None).unwrap());
        let docs = vec![json!({"x": "a"}), json!({"x": "b"}), json!({"x": "c"})];

        let result = exporter
            .send("test-index".to_string(), docs)
            .await
            .expect("byte split send");

        assert_eq!(result.batch_count, 3);
        assert_eq!(result.docs, 3);
        assert_eq!(mock.bulk_requests().len(), 3, "expected three bulk requests");
    }

    #[tokio::test]
    async fn exporter_send_allows_disabling_byte_splitting() {
        let _lock = ENV_LOCK.lock().await;
        let _env = BulkBytesEnvGuard::set("0");
        let (url, mock) = mock_bulk_server(vec![]).await;
        let exporter =
            crate::exporter::Exporter::Elasticsearch(ElasticsearchExporter::try_new(url, Auth::None).unwrap());
        let docs = vec![json!({"x": "a"}), json!({"x": "b"}), json!({"x": "c"})];

        let result = exporter
            .send("test-index".to_string(), docs)
            .await
            .expect("byte-disabled send");

        assert_eq!(result.batch_count, 1);
        assert_eq!(result.docs, 3);
        assert_eq!(mock.bulk_requests().len(), 1, "expected one bulk request");
    }

    #[tokio::test]
    async fn exporter_send_reports_single_batch_send_failure() {
        let _lock = ENV_LOCK.lock().await;
        let _env = BulkBytesEnvGuard::set("0");
        let (url, mock) = mock_bulk_server(vec![BulkReply::Status(413)]).await;
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
        assert_eq!(response.status_code, BatchResponse::HTTP_REQUEST_NOT_COMPLETED);
        assert_eq!(mock.bulk_requests().len(), 1, "expected one bulk request");
    }

    async fn wait_for_bulk_requests(mock: &ElasticsearchMock, count: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while mock.bulk_requests().len() < count {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("bulk request reached the mock");
    }

    fn scoped_exporter(url: Url, scope: &ExecutionScope) -> ElasticsearchExporter {
        ElasticsearchExporter::try_new(url, Auth::None)
            .unwrap()
            .with_scope(scope.clone())
    }

    #[tokio::test]
    async fn a_cancelled_export_sends_no_further_sub_batch() {
        let _lock = ENV_LOCK.lock().await;
        let _env = BulkBytesEnvGuard::set("0");
        let release = Arc::new(tokio::sync::Notify::new());
        let (url, mock) = mock_bulk_server(vec![BulkReply::Held(release.clone())]).await;
        let scope = ExecutionScope::default();
        let exporter = scoped_exporter(url, &scope);
        let docs = (0..=crate::env::ESDIAG_ES_BULK_SIZE)
            .map(|id| json!({ "x": id }))
            .collect::<Vec<_>>();

        let send = tokio::spawn(async move { exporter.batch_send("test-index".to_string(), docs).await });
        wait_for_bulk_requests(&mock, 1).await;
        scope.token().cancel();
        release.notify_one();
        let response = send.await.unwrap().expect("the first sub-batch was answered");

        assert_eq!(
            mock.bulk_requests().len(),
            1,
            "no sub-batch may start after cancellation"
        );
        assert_eq!(response.docs as usize, crate::env::ESDIAG_ES_BULK_SIZE);
        assert_eq!(response.errors, 1, "the unsent sub-batch counts as failed");
    }

    #[tokio::test]
    async fn cancelling_during_a_request_429_backoff_sends_no_retry() {
        let _lock = ENV_LOCK.lock().await;
        let _env = RetryEnvGuard::with_backoff("5", "60000", "60000");
        let (url, mock) = mock_bulk_server(vec![BulkReply::Status(429)]).await;
        let scope = ExecutionScope::default();
        let exporter = scoped_exporter(url, &scope);

        let send = tokio::spawn(async move {
            exporter
                .batch_send("test-index".to_string(), vec![json!({"x": 1})])
                .await
        });
        wait_for_bulk_requests(&mock, 1).await;
        scope.token().cancel();
        let result = tokio::time::timeout(Duration::from_secs(5), send)
            .await
            .expect("cancellation ends the backoff")
            .unwrap();

        assert!(result.is_err(), "nothing was answered, so the batch fails");
        assert_eq!(mock.bulk_requests().len(), 1, "no retry after cancellation");
    }

    #[tokio::test]
    async fn cancelling_before_a_resend_keeps_created_documents() {
        let _lock = ENV_LOCK.lock().await;
        let _env = RetryEnvGuard::with_backoff("5", "60000", "60000");
        let mut test_env = crate::TestEnv::new();
        test_env.set_path("ESDIAG_HOME", test_env.tmp.path().to_path_buf());
        std::fs::create_dir_all(test_env.tmp.path().join("last_run")).unwrap();
        let first = BulkReply::Items(vec![created_item("a"), throttled_item()]);
        let (url, mock) = mock_bulk_server(vec![first]).await;
        let scope = ExecutionScope::default();
        let exporter = scoped_exporter(url, &scope);
        let docs = vec![json!({"n": 1}), json!({"n": 2})];

        let send = tokio::spawn(async move { exporter.batch_send("test-esdiag".to_string(), docs).await });
        wait_for_bulk_requests(&mock, 1).await;
        scope.token().cancel();
        let response = tokio::time::timeout(Duration::from_secs(5), send)
            .await
            .expect("cancellation ends the backoff")
            .unwrap()
            .expect("a document was created");

        assert_eq!((response.docs, response.errors), (1, 1));
        assert_eq!(mock.bulk_requests().len(), 1, "no resend after cancellation");
    }

    #[tokio::test]
    async fn the_tracker_waits_for_a_request_in_flight() {
        let _lock = ENV_LOCK.lock().await;
        let release = Arc::new(tokio::sync::Notify::new());
        let (url, mock) = mock_bulk_server(vec![BulkReply::Held(release.clone())]).await;
        let scope = ExecutionScope::default();
        let exporter = scoped_exporter(url, &scope);

        let response = exporter
            .batch_tx("test-index".to_string(), vec![json!({"x": 1})])
            .await
            .expect("batch queued");
        wait_for_bulk_requests(&mock, 1).await;
        scope.token().cancel();
        scope.tracker().close();

        assert!(
            tokio::time::timeout(Duration::from_millis(100), scope.tracker().wait())
                .await
                .is_err(),
            "the request is still in flight"
        );
        release.notify_one();
        tokio::time::timeout(Duration::from_secs(5), scope.tracker().wait())
            .await
            .expect("the tracker empties once the request ends");
        assert_eq!(response.await.expect("batch answered").docs, 1);
    }

    #[tokio::test]
    async fn a_dropped_send_keeps_its_request_in_the_tracker() {
        let _lock = ENV_LOCK.lock().await;
        let release = Arc::new(tokio::sync::Notify::new());
        let (url, mock) = mock_bulk_server(vec![BulkReply::Held(release.clone())]).await;
        let scope = ExecutionScope::default();
        let exporter = scoped_exporter(url, &scope);

        let send = tokio::spawn(async move {
            exporter
                .batch_send("test-index".to_string(), vec![json!({"x": 1})])
                .await
        });
        wait_for_bulk_requests(&mock, 1).await;
        send.abort();
        let _ = send.await;
        scope.token().cancel();
        scope.tracker().close();

        assert!(
            tokio::time::timeout(Duration::from_millis(100), scope.tracker().wait())
                .await
                .is_err(),
            "the request outlives its dropped caller"
        );
        release.notify_one();
        tokio::time::timeout(Duration::from_secs(5), scope.tracker().wait())
            .await
            .expect("the tracker empties once the request ends");
    }

    #[tokio::test]
    async fn a_cancelled_batch_stops_waiting_for_a_permit() {
        let _lock = ENV_LOCK.lock().await;
        let (url, mock) = mock_bulk_server(vec![]).await;
        let scope = ExecutionScope::default();
        let exporter = scoped_exporter(url, &scope);
        let _held = exporter
            .tx_limit
            .clone()
            .acquire_many_owned(exporter.tx_limit.available_permits() as u32)
            .await
            .expect("hold every permit");

        let response = exporter
            .batch_tx("test-index".to_string(), vec![json!({"x": 1})])
            .await
            .expect("batch queued");
        scope.token().cancel();
        scope.tracker().close();

        tokio::time::timeout(Duration::from_secs(5), scope.tracker().wait())
            .await
            .expect("the queued batch leaves the tracker without a permit");
        assert!(response.await.is_err(), "the batch was never answered");
        assert!(mock.bulk_requests().is_empty());
    }
}
