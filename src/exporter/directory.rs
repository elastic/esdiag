// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::Export;
use crate::data::Uri;
use crate::processor::{BatchResponse, DiagnosticReport};
use eyre::{Result, eyre};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};
use tokio::sync::{RwLock, mpsc, oneshot};

type Writers = HashMap<String, Arc<RwLock<BufWriter<File>>>>;

fn create_writer(path: &Path, index: &str) -> Result<Arc<RwLock<BufWriter<File>>>> {
    let filename = format!("{}.ndjson", index);
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path.join(filename))?;
    Ok(Arc::new(RwLock::new(BufWriter::new(file))))
}

async fn get_writer(
    writers: Arc<RwLock<Writers>>,
    path: &Path,
    index: &str,
) -> Result<Arc<RwLock<BufWriter<File>>>> {
    let mut writers = writers.write().await;
    if let Some(writer) = writers.get(index) {
        Ok(writer.clone())
    } else {
        let writer = create_writer(path, index)?;
        writers.insert(index.to_string(), writer.clone());
        Ok(writer)
    }
}

#[derive(Clone)]
pub struct DirectoryExporter {
    path: PathBuf,
    docs_tx: Option<mpsc::Sender<usize>>,
    writers: Arc<RwLock<Writers>>,
}

impl DirectoryExporter {
    pub fn is_dir(&self) -> bool {
        self.path.is_dir()
    }

    pub async fn save(&self, path: PathBuf, content: String) -> Result<()> {
        let path = &self.path.join(path);
        tracing::debug!("Writing file: {}", &path.display());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(path)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }

    pub fn collection_directory(mut self, directory: String) -> Result<Self> {
        tracing::debug!("Creating directory: {}", &directory);
        self.path = self.path.join(directory);
        std::fs::create_dir_all(&self.path)?;
        Ok(self)
    }
}

impl TryFrom<Uri> for DirectoryExporter {
    type Error = eyre::Report;

    fn try_from(uri: Uri) -> Result<Self> {
        match uri {
            Uri::Directory(path) => Self::try_from(path),
            Uri::File(path) => Self::try_from(path),
            _ => Err(eyre!("Expected directory got {}", uri.to_string())),
        }
    }
}

impl TryFrom<PathBuf> for DirectoryExporter {
    type Error = eyre::Report;

    fn try_from(path: PathBuf) -> Result<Self> {
        if path.exists() {
            if !path.is_dir() {
                return Err(eyre!(
                    "Directory output destination must be a directory: {}",
                    path.display()
                ));
            }
        } else {
            tracing::debug!("Creating directory: {}", path.display());
            std::fs::create_dir_all(&path)?;
        }

        Ok(Self {
            path,
            docs_tx: None,
            writers: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

impl std::fmt::Display for DirectoryExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.path.display())
    }
}

impl Export for DirectoryExporter {
    fn get_docs_rx(&mut self) -> mpsc::Receiver<usize> {
        let (tx, rx) = mpsc::channel::<usize>(100);
        self.docs_tx = Some(tx);
        rx
    }

    /// Validates the file path and returns true if it exists.
    async fn is_connected(&self) -> bool {
        let is_dir = self.path.is_dir();
        let dir_name = self.path.to_str().unwrap_or("");
        tracing::debug!("Directory {dir_name} is valid: {is_dir}");
        is_dir
    }

    /// Drains the vec and writes all documents to a file in the directory.
    async fn batch_send<T>(&self, index: String, docs: Vec<T>) -> Result<BatchResponse>
    where
        T: Sized + Serialize,
    {
        let start_time = tokio::time::Instant::now();
        let mut batch = BatchResponse::new(docs.len() as u32);
        let mut doc_count = 0;
        {
            let writer = get_writer(self.writers.clone(), &self.path, &index).await?;
            let mut writer = writer.write().await;
            for doc in docs {
                serde_json::to_writer(&mut *writer, &doc)?;
                writeln!(&mut writer)?;
                doc_count += 1;
            }
            writer.flush()?;
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
    async fn batch_tx<T>(
        &self,
        index: String,
        docs: Vec<T>,
    ) -> Result<oneshot::Receiver<BatchResponse>>
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

    /// Saves the final diagnostic report file to output directory
    async fn save_report(&self, report: &DiagnosticReport) -> Result<()> {
        std::fs::write(
            self.path.join("metrics-diagnostic-esdiag.ndjson"),
            serde_json::to_string(report)?,
        )
        .map_err(|e| eyre::eyre!("Failed to save report: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::DirectoryExporter;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn directory_exporter_rejects_existing_file_path() {
        let dir = tempdir().expect("temp dir");
        let file_path = dir.path().join("not-a-dir");
        File::create(&file_path).expect("create file");

        let err = DirectoryExporter::try_from(file_path)
            .err()
            .expect("expected non-directory path to fail");
        assert!(err.to_string().contains("must be a directory"));
    }
}
