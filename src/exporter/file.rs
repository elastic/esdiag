// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::Export;
use crate::processor::{BatchResponse, DiagnosticReport};
use eyre::Result;
use serde::Serialize;
use std::sync::{Arc, RwLock};
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
};
use tokio::sync::{mpsc, oneshot};

/// An exporter that writes to a file.
pub struct FileExporter {
    file: File,
    path: PathBuf,
    docs_tx: Option<mpsc::Sender<usize>>,
    writer: Arc<RwLock<BufWriter<File>>>,
}

impl Clone for FileExporter {
    fn clone(&self) -> Self {
        Self {
            file: self.file.try_clone().expect("Failed to clone file"),
            path: self.path.clone(),
            writer: self.writer.clone(),
            docs_tx: self.docs_tx.clone(),
        }
    }
}

impl TryFrom<PathBuf> for FileExporter {
    type Error = eyre::Report;

    fn try_from(path: PathBuf) -> Result<Self> {
        match path.is_file() {
            false => {
                tracing::info!("Creating file {}", path.display());
                File::create(&path)?;
            }
            true => {
                tracing::debug!("File {} exists", path.display());
            }
        }

        let file = OpenOptions::new().create(true).truncate(true).write(true).open(&path)?;

        Ok(Self {
            file: file.try_clone().expect("Failed to clone file"),
            path,
            writer: Arc::new(RwLock::new(BufWriter::new(file))),
            docs_tx: None,
        })
    }
}

impl Export for FileExporter {
    fn get_docs_rx(&mut self) -> mpsc::Receiver<usize> {
        let (tx, rx) = mpsc::channel::<usize>(100);
        self.docs_tx = Some(tx);
        rx
    }

    /// Validates the file path and returns true if it exists.
    async fn is_connected(&self) -> bool {
        let is_file = self.path.is_file();
        let filename = self.path.to_str().unwrap_or("");
        tracing::debug!("File {filename} is valid: {is_file}");
        is_file
    }

    /// Drains the vec and writes all documents to the file.
    async fn batch_send<T>(&self, index: String, docs: Vec<T>) -> Result<BatchResponse>
    where
        T: Sized + Serialize,
    {
        let start_time = tokio::time::Instant::now();
        let mut batch = BatchResponse::new(docs.len() as u32);
        let mut doc_count = 0;
        {
            let mut writer = self
                .writer
                .write()
                .map_err(|e| eyre::eyre!("Failed to acquire write lock: {}", e))?;
            for doc in docs {
                serde_json::to_writer(&mut *writer, &doc)?;
                writeln!(&mut writer)?;
                doc_count += 1;
            }
            writer.flush()?;
        }
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::MetadataExt;
            batch.size = self.file.metadata()?.size() as u32;
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::MetadataExt;
            batch.size = self.file.metadata()?.size() as u32;
        }
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::fs::MetadataExt;
            batch.size = self.file.metadata()?.file_size() as u32;
        }
        batch.time = start_time.elapsed().as_millis() as u32;

        tracing::info!("{}, created {} docs", index, doc_count);
        if let Some(tx) = &self.docs_tx {
            let _ = tx.send(doc_count).await;
        }
        Ok(batch)
    }

    /// Transmits a single batch of documents in an async task
    /// Returns a one-shot channel for the BatchResponse
    async fn batch_tx<T>(&self, index: String, docs: Vec<T>) -> Result<oneshot::Receiver<BatchResponse>>
    where
        T: Serialize + Sized + Send + Sync + 'static,
    {
        let (tx, rx) = oneshot::channel();

        // File exporter writes synchronously, so we just write and send a simple response
        match self.batch_send(index, docs).await {
            Ok(batch_response) => {
                if tx.send(batch_response).is_err() {
                    tracing::error!("Failed to send batch response");
                }
            }
            Err(e) => tracing::warn!("File write failed: {}", e),
        }

        Ok(rx)
    }

    /// Saves the final diagnostic report file to the esdiag home directory
    async fn save_report(&self, report: &DiagnosticReport) -> Result<()> {
        crate::data::save_file("report.json", report)
    }
}

impl std::fmt::Display for FileExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.path.display())
    }
}
