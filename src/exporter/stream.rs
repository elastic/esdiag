// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::Export;
use crate::processor::{BatchResponse, DiagnosticReport, Identifiers, ProcessorSummary};
use eyre::Result;
use serde::Serialize;
use tokio::sync::oneshot;

/// An exporter that writes documents to stdout.
#[derive(Clone)]
pub struct StreamExporter {
    pub identifiers: Identifiers,
}

impl StreamExporter {
    pub fn new() -> Self {
        Self {
            identifiers: Identifiers::default(),
        }
    }
}

impl Export for StreamExporter {
    /// Adds identifiers to the exporter, which will be enriched on every document sent.
    fn with_identifiers(self, identifiers: Identifiers) -> Self {
        Self {
            identifiers,
            ..self
        }
    }

    /// Returns true for compatibility, can stdout not exist?
    async fn is_connected(&self) -> bool {
        true
    }

    /// Drains the vec and writes all documents to the file.
    async fn send<T>(&self, summary: &mut ProcessorSummary, docs: &mut Vec<T>) -> Result<()>
    where
        T: Sized + Serialize,
    {
        let doc_count = docs.len() as u32;
        let start_time = std::time::Instant::now();
        let mut batch = BatchResponse::new(doc_count);
        log::debug!("Writing {} docs to stdout", doc_count);
        for doc in docs {
            serde_json::to_writer(std::io::stdout(), &doc)?;
            println!();
        }
        batch.size = doc_count;
        batch.time = start_time.elapsed().as_secs() as u32;
        batch.time = start_time.elapsed().as_millis() as u32;
        summary.add_batch(batch);
        Ok(())
    }

    /// Transmits a single batch of documents in an async task
    /// Returns a one-shot channel for the BatchResponse
    async fn tx<T>(&self, index: String, docs: Vec<T>) -> Result<oneshot::Receiver<BatchResponse>>
    where
        T: Serialize + Sized + Send + Sync + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let doc_count = docs.len() as u32;
        let mut temp_docs = docs;
        let mut summary = ProcessorSummary::new(index);

        // Stream exporter writes synchronously, so we just write and send a simple response
        match self.send(&mut summary, &mut temp_docs).await {
            Ok(_) => {
                let batch_response = BatchResponse::new(doc_count);
                let _ = tx.send(batch_response);
            }
            Err(e) => {
                log::warn!("Stream write failed: {}", e);
            }
        }

        Ok(rx)
    }

    /// Saves the final diagnostic report file to the esdiag home directory
    async fn save_report(&self, report: &DiagnosticReport) -> Result<()> {
        crate::data::save_file("report.json", report)
    }
}

impl std::fmt::Display for StreamExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "stdout")
    }
}
