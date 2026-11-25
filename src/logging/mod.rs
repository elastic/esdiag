// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

pub mod config;
pub mod file;
pub mod layer;
pub mod worker;

pub use config::{DEFAULT_LOGS_DATA_STREAM, LogsTargetConfig, LogsTargetSource, resolve_data_stream};
pub use file::{DailyLogMaintenance, DailyLogWriter};
pub use layer::ElasticsearchLogLayer;

use std::time::Duration;
use worker::{DEFAULT_QUEUE_CAPACITY, WorkerConfig, WorkerHandle, spawn_worker};

/// Default bounded shutdown wait for flushing remote logs on process exit.
pub const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/// Guard returned from tracing initialization to flush buffered remote logs
/// and run file-log exit maintenance (gzip + retention pruning) before exit.
#[derive(Default)]
pub struct LogsGuard {
    remote: Option<WorkerHandle>,
    file: Option<DailyLogMaintenance>,
}

impl LogsGuard {
    pub fn new(remote: Option<WorkerHandle>, file: Option<DailyLogMaintenance>) -> Self {
        Self { remote, file }
    }

    /// Flushes remote log batches (up to `timeout`) and runs file log exit
    /// compression and retention cleanup. Safe to call even when disabled.
    pub async fn shutdown(mut self, timeout: Duration) {
        if let Some(remote) = self.remote.take() {
            remote.flush_and_stop(timeout).await;
        }
        if let Some(file) = self.file.take() {
            let _ = tokio::task::spawn_blocking(move || file.run_on_exit()).await;
        }
    }
}

/// Resolves the remote Elasticsearch logs target and, if configured, spawns
/// its background batch worker and returns the non-blocking tracing layer and
/// worker handle.
///
/// Configuration errors are printed to stderr and degrade gracefully to `None`.
pub fn init_remote_logs_layer() -> (Option<ElasticsearchLogLayer>, Option<WorkerHandle>) {
    match LogsTargetConfig::resolve() {
        Ok(Some(target)) => {
            let (tx, dropped, handle) = spawn_worker(
                target.client,
                target.data_stream,
                DEFAULT_QUEUE_CAPACITY,
                WorkerConfig::default(),
            );
            (Some(ElasticsearchLogLayer::new(tx, dropped)), Some(handle))
        }
        Ok(None) => (None, None),
        Err(err) => {
            eprintln!("Remote logs exporter disabled: {err}");
            (None, None)
        }
    }
}
