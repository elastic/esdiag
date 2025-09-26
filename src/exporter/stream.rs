// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::Export;
use crate::processor::{BatchResponse, DiagnosticReport};
use eyre::Result;
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

/// An exporter that writes documents to stdout.
#[derive(Clone)]
pub struct StreamExporter {
    docs_tx: Option<mpsc::Sender<usize>>,
}

impl StreamExporter {
    pub fn new() -> Self {
        StreamExporter { docs_tx: None }
    }
}

impl Export for StreamExporter {
    fn get_docs_rx(&mut self) -> mpsc::Receiver<usize> {
        let (tx, rx) = mpsc::channel::<usize>(100);
        self.docs_tx = Some(tx);
        rx
    }

    /// Returns true for compatibility, can stdout not exist?
    async fn is_connected(&self) -> bool {
        true
    }

    /// Writes the docs to stdout
    async fn batch_send<T>(&self, index: String, docs: Vec<T>) -> Result<BatchResponse>
    where
        T: Serialize + Sized + Send + Sync,
    {
        let start_time = tokio::time::Instant::now();
        let doc_count = docs.len() as u32;
        let mut batch = BatchResponse::new(doc_count);
        log::debug!("{} wrote {} docs to stdout", index, doc_count);
        for doc in docs {
            serde_json::to_writer(std::io::stdout(), &doc)?;
            println!();
        }
        batch.size = doc_count;
        batch.time = start_time.elapsed().as_millis() as u32;
        Ok(batch)
    }

    /// Transmits a single batch of documents in an async task
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

        // Stream exporter writes synchronously, so we just write and send the response
        match self.batch_send(index, docs).await {
            Ok(batch_response) => {
                if tx.send(batch_response).is_err() {
                    log::error!("Failed to send batch response");
                }
            }
            Err(e) => log::warn!("Stream write failed: {}", e),
        }

        Ok(rx)
    }

    /// Writes the final diagnostic report file to stdout
    async fn save_report(&self, report: &DiagnosticReport) -> Result<()> {
        println!("{}", serde_json::to_string(report)?);
        Ok(())
    }
}

impl std::fmt::Display for StreamExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "stdout")
    }
}
