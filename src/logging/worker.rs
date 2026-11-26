// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use elasticsearch::{BulkOperation, BulkParts};
use esdiag_core::client::ElasticsearchClient;
use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{interval, timeout};

/// Bounded channel capacity for pending remote log documents.
pub const DEFAULT_QUEUE_CAPACITY: usize = 4096;
/// Maximum number of log documents sent in a single `_bulk` request.
pub const DEFAULT_BATCH_SIZE: usize = 100;
/// Maximum time buffered events wait before being flushed.
pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_secs(2);
/// Per-request HTTP timeout for `_bulk` delivery.
const BULK_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// Minimum interval between repeated stderr delivery failure warnings.
const ERROR_WARN_INTERVAL: Duration = Duration::from_secs(30);

pub(crate) struct WorkerConfig {
    pub batch_size: usize,
    pub flush_interval: Duration,
    pub request_timeout: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
            flush_interval: DEFAULT_FLUSH_INTERVAL,
            request_timeout: BULK_REQUEST_TIMEOUT,
        }
    }
}

pub struct WorkerHandle {
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
    dropped: Arc<AtomicU64>,
}

impl WorkerHandle {
    pub(crate) async fn flush_and_stop(mut self, max_wait: Duration) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(mut handle) = self.join_handle.take()
            && timeout(max_wait, &mut handle).await.is_err()
        {
            handle.abort();
            let _ = handle.await;
            eprintln!(
                "Remote logs exporter flush timed out after {}ms; some log events may not have been delivered",
                max_wait.as_millis()
            );
        }
        let dropped = self.dropped.load(Ordering::Relaxed);
        if dropped > 0 {
            eprintln!("Remote logs exporter dropped {dropped} log event(s) because the queue was full");
        }
    }
}

pub(crate) fn spawn_worker(
    client: ElasticsearchClient,
    data_stream: String,
    queue_capacity: usize,
    config: WorkerConfig,
) -> (mpsc::Sender<Value>, Arc<AtomicU64>, WorkerHandle) {
    let (tx, rx) = mpsc::channel::<Value>(queue_capacity);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let dropped = Arc::new(AtomicU64::new(0));

    let join_handle = tokio::spawn(run_worker(client, data_stream, rx, shutdown_rx, config));

    let handle = WorkerHandle {
        shutdown_tx: Some(shutdown_tx),
        join_handle: Some(join_handle),
        dropped: Arc::clone(&dropped),
    };
    (tx, dropped, handle)
}

async fn run_worker(
    client: ElasticsearchClient,
    data_stream: String,
    mut rx: mpsc::Receiver<Value>,
    mut shutdown_rx: oneshot::Receiver<()>,
    config: WorkerConfig,
) {
    let mut batch: Vec<Value> = Vec::with_capacity(config.batch_size);
    let mut ticker = interval(config.flush_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Consume the immediate first tick so we don't spin on startup.
    ticker.tick().await;
    let mut last_warn: Option<Instant> = None;

    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown_rx => {
                drain_and_flush(&client, &data_stream, &mut rx, &mut batch, &config, &mut last_warn).await;
                break;
            }
            maybe_doc = rx.recv() => {
                match maybe_doc {
                    Some(doc) => {
                        batch.push(doc);
                        if batch.len() >= config.batch_size {
                            flush_batch(&client, &data_stream, &mut batch, config.request_timeout, &mut last_warn).await;
                        }
                    }
                    None => {
                        if !batch.is_empty() {
                            flush_batch(&client, &data_stream, &mut batch, config.request_timeout, &mut last_warn).await;
                        }
                        break;
                    }
                }
            }
            _ = ticker.tick() => {
                if !batch.is_empty() {
                    flush_batch(&client, &data_stream, &mut batch, config.request_timeout, &mut last_warn).await;
                }
            }
        }
    }
}

async fn drain_and_flush(
    client: &ElasticsearchClient,
    data_stream: &str,
    rx: &mut mpsc::Receiver<Value>,
    batch: &mut Vec<Value>,
    config: &WorkerConfig,
    last_warn: &mut Option<Instant>,
) {
    rx.close();
    while let Some(doc) = rx.recv().await {
        batch.push(doc);
        if batch.len() >= config.batch_size {
            flush_batch(client, data_stream, batch, config.request_timeout, last_warn).await;
        }
    }
    if !batch.is_empty() {
        flush_batch(client, data_stream, batch, config.request_timeout, last_warn).await;
    }
}

async fn flush_batch(
    client: &ElasticsearchClient,
    data_stream: &str,
    batch: &mut Vec<Value>,
    request_timeout: Duration,
    last_warn: &mut Option<Instant>,
) {
    let docs = std::mem::take(batch);
    if docs.is_empty() {
        return;
    }

    let operations: Vec<BulkOperation<Value>> = docs.into_iter().map(|doc| BulkOperation::create(doc).into()).collect();

    let result = timeout(request_timeout, async {
        let response = client
            .bulk(BulkParts::Index(data_stream))
            .body(operations)
            .send()
            .await?;
        let status = response.status_code();
        if !status.is_success() {
            return Ok::<_, elasticsearch::Error>((status, None));
        }
        let body = response.json::<Value>().await?;
        Ok((status, Some(body)))
    })
    .await;

    match result {
        Ok(Ok((status, _))) if !status.is_success() => {
            warn_rate_limited(
                last_warn,
                &format!("Remote logs exporter received HTTP {status} writing to '{data_stream}'"),
            );
        }
        Ok(Ok((_, Some(body)))) => {
            if let Some(summary) = summarize_bulk_item_errors(&body) {
                warn_rate_limited(
                    last_warn,
                    &format!("Remote logs exporter bulk item errors writing to '{data_stream}': {summary}"),
                );
            }
        }
        Ok(Ok((_, None))) => {}
        Ok(Err(err)) => {
            warn_rate_limited(
                last_warn,
                &format!("Remote logs exporter failed to deliver batch to '{data_stream}': {err}"),
            );
        }
        Err(_) => {
            warn_rate_limited(
                last_warn,
                &format!(
                    "Remote logs exporter timed out after {}ms writing to '{data_stream}'",
                    request_timeout.as_millis()
                ),
            );
        }
    }
}

fn summarize_bulk_item_errors(body: &Value) -> Option<String> {
    if !body.get("errors").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    let items = body.get("items").and_then(Value::as_array)?;
    let mut failed = 0usize;
    let mut first_detail: Option<String> = None;
    for item in items {
        let Some(entry) = item.as_object().and_then(|obj| obj.values().next()) else {
            continue;
        };
        let status = entry.get("status").and_then(Value::as_u64).unwrap_or(0);
        let has_error = entry.get("error").is_some() || status >= 300;
        if has_error {
            failed += 1;
            if first_detail.is_none() {
                let reason = entry
                    .get("error")
                    .and_then(|err| err.get("reason").and_then(Value::as_str).or_else(|| err.as_str()))
                    .unwrap_or("unknown error");
                first_detail = Some(format!("status {status} ({reason})"));
            }
        }
    }
    if failed == 0 {
        return Some("errors=true with no failing item details".to_string());
    }
    Some(format!(
        "{failed} item(s) rejected (first: {})",
        first_detail.unwrap_or_else(|| "unknown".to_string())
    ))
}

fn warn_rate_limited(last_warn: &mut Option<Instant>, message: &str) {
    let now = Instant::now();
    let should_warn = last_warn.is_none_or(|prev| now.duration_since(prev) >= ERROR_WARN_INTERVAL);
    if should_warn {
        eprintln!("{message}");
        *last_warn = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkerConfig, spawn_worker};
    use esdiag_core::client::ElasticsearchBuilder;
    use esdiag_core::es_mock::{BulkReply, ElasticsearchMock};
    use serde_json::json;
    use std::time::Duration;
    use url::Url;

    #[tokio::test]
    async fn batches_and_flushes_create_operations_to_data_stream_with_auth() {
        let mock = ElasticsearchMock::new().require_authorization("ApiKey test-logs-key");
        let base_url = mock.serve().await;

        let client = ElasticsearchBuilder::new(Url::parse(&base_url).expect("mock url"))
            .apikey("test-logs-key".to_string())
            .build()
            .expect("client");

        let (tx, _dropped, handle) = spawn_worker(
            client,
            "logs-esdiag-default".to_string(),
            64,
            WorkerConfig {
                batch_size: 2,
                flush_interval: Duration::from_secs(60),
                request_timeout: Duration::from_secs(2),
            },
        );

        for i in 0..3 {
            tx.try_send(json!({
                "@timestamp": "2026-11-24T00:00:00Z",
                "message": format!("msg-{i}")
            }))
            .expect("enqueue");
        }

        handle.flush_and_stop(Duration::from_secs(3)).await;

        let requests = mock.bulk_requests();
        assert_eq!(requests.len(), 2, "2 docs in first batch + 1 drained on shutdown");
        assert_eq!(requests[0].path_index, "logs-esdiag-default");
        assert_eq!(requests[0].authorization.as_deref(), Some("ApiKey test-logs-key"));
        assert_eq!(requests[0].actions.len(), 2);
        assert!(
            requests[0]
                .actions
                .iter()
                .all(|a| a.op == "create" && a.pipeline.is_none())
        );
        assert_eq!(requests[1].actions.len(), 1);
        assert_eq!(requests[1].documents[0]["message"], "msg-2");
    }

    #[tokio::test]
    async fn periodic_flush_delivers_partial_batch() {
        let mock = ElasticsearchMock::new();
        let base_url = mock.serve().await;
        let client = ElasticsearchBuilder::new(Url::parse(&base_url).expect("mock url"))
            .build()
            .expect("client");

        let (tx, _dropped, handle) = spawn_worker(
            client,
            "logs-esdiag-periodic".to_string(),
            64,
            WorkerConfig {
                batch_size: 100,
                flush_interval: Duration::from_millis(50),
                request_timeout: Duration::from_secs(2),
            },
        );

        tx.try_send(json!({"@timestamp": "2026-11-24T00:00:00Z", "message": "periodic"}))
            .expect("enqueue");

        tokio::time::sleep(Duration::from_millis(180)).await;
        assert_eq!(mock.bulk_requests().len(), 1);
        assert_eq!(mock.bulk_requests()[0].path_index, "logs-esdiag-periodic");

        handle.flush_and_stop(Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn http_500_and_unreachable_endpoint_do_not_panic_or_hang() {
        let mock = ElasticsearchMock::new().bulk_replies([BulkReply::Status(500)]);
        let base_url = mock.serve().await;
        let client = ElasticsearchBuilder::new(Url::parse(&base_url).expect("mock url"))
            .build()
            .expect("client");

        let (tx, _dropped, handle) = spawn_worker(
            client,
            "logs-esdiag-default".to_string(),
            16,
            WorkerConfig {
                batch_size: 1,
                flush_interval: Duration::from_secs(10),
                request_timeout: Duration::from_secs(2),
            },
        );
        tx.try_send(json!({"@timestamp": "2026-11-24T00:00:00Z", "message": "will-500"}))
            .expect("enqueue");
        handle.flush_and_stop(Duration::from_secs(2)).await;
        assert_eq!(mock.bulk_requests().len(), 1);

        let unreachable_client = ElasticsearchBuilder::new(Url::parse("http://127.0.0.1:1").expect("url"))
            .build()
            .expect("client");
        let (tx, _dropped, handle) = spawn_worker(
            unreachable_client,
            "logs-esdiag-default".to_string(),
            16,
            WorkerConfig {
                batch_size: 1,
                flush_interval: Duration::from_secs(10),
                request_timeout: Duration::from_millis(500),
            },
        );
        tx.try_send(json!({"@timestamp": "2026-11-24T00:00:00Z", "message": "unreachable"}))
            .expect("enqueue");
        handle.flush_and_stop(Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn bulk_item_errors_are_detected_without_failing_worker() {
        let mock = ElasticsearchMock::new().bulk_replies([BulkReply::Items(vec![json!({
            "_index": "logs-esdiag-default",
            "status": 429,
            "error": { "type": "es_rejected_execution_exception", "reason": "queue full" }
        })])]);
        let base_url = mock.serve().await;
        let client = ElasticsearchBuilder::new(Url::parse(&base_url).expect("mock url"))
            .build()
            .expect("client");

        let mut batch = vec![json!({"@timestamp": "2026-11-24T00:00:00Z", "message": "item-err"})];
        let mut last_warn = None;
        super::flush_batch(
            &client,
            "logs-esdiag-default",
            &mut batch,
            Duration::from_secs(2),
            &mut last_warn,
        )
        .await;

        assert!(
            last_warn.is_some(),
            "HTTP 200 bulk response with errors=true must trigger rate-limited warning"
        );
    }

    #[tokio::test]
    async fn flush_and_stop_aborts_worker_on_timeout() {
        let (shutdown_tx, _shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let finished_flag = std::sync::Arc::clone(&finished);
        let join_handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            finished_flag.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        let abort_check = join_handle.abort_handle();
        let handle = super::WorkerHandle {
            shutdown_tx: Some(shutdown_tx),
            join_handle: Some(join_handle),
            dropped: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        };

        handle.flush_and_stop(Duration::from_millis(20)).await;
        assert!(
            abort_check.is_finished(),
            "timed-out worker task must be aborted and awaited"
        );
        assert!(!finished.load(std::sync::atomic::Ordering::SeqCst));
    }
}
