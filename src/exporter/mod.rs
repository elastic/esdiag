// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Write collection output to a zip archive
mod archive;
/// Write collection output to a directory
mod directory;
/// Send to an Elasticsearch cluster with the `_bulk` API
mod elasticsearch;
/// Write to an `.ndjson` file
mod file;
/// Write `ndjson` to std out
mod stream;

use crate::{
    data::{KnownHost, Product, Uri},
    processor::{BatchResponse, DiagnosticReport, ProcessorSummary},
};
pub use archive::ArchiveExporter;
pub use directory::DirectoryExporter;
use elasticsearch::ElasticsearchExporter;
use eyre::{Result, eyre};
use file::FileExporter;
use serde::Serialize;
use std::path::PathBuf;
use stream::StreamExporter;
use tokio::sync::{mpsc, oneshot};

trait Export {
    async fn is_connected(&self) -> bool;
    async fn batch_send<T>(&self, index: String, docs: Vec<T>) -> Result<BatchResponse>
    where
        T: Serialize + Sized + Send + Sync;
    async fn batch_tx<T>(
        &self,
        index: String,
        docs: Vec<T>,
    ) -> Result<oneshot::Receiver<BatchResponse>>
    where
        T: Serialize + Sized + Send + Sync + 'static;
    async fn save_report(&self, report: &DiagnosticReport) -> Result<()>;
    fn get_docs_rx(&mut self) -> mpsc::Receiver<usize>;
}

/// The different types of exporters for data output.
///
/// This enum encapsulates various implementations of the `Export` trait,
/// allowing for flexible handling of different data sources. Each variant
/// corresponds to a specific method of data output:
///
/// - `Elasticsearch`: Exports data to an Elasticsearch cluster using the `_bulk` API.
/// - `File`: Exports data to a `.ndjson` file.
/// - `Stream`: Exports data to standard output (stdout).
#[derive(Clone)]
pub enum Exporter {
    /// Export collection artifacts to a directory or zip archive
    Archive(ArchiveExporter),
    /// Export to an Elasticsearch cluster with the `_bulk` API
    Elasticsearch(ElasticsearchExporter),
    /// Export to an `.ndjson` file
    File(FileExporter),
    /// Export to a directory of `${index}.ndjson` files
    Directory(DirectoryExporter),
    /// Export to `stdout`
    Stream(StreamExporter),
}

impl Exporter {
    pub fn for_collect(uri: Uri) -> Result<Self> {
        match uri {
            Uri::Directory(path) | Uri::File(path) => {
                Ok(Self::Directory(DirectoryExporter::try_from(path)?))
            }
            _ => Err(eyre!(
                "Collect requires a local directory output when --zip is not set"
            )),
        }
    }

    pub fn for_collect_archive(output_dir: PathBuf) -> Result<Self> {
        Ok(Self::Archive(ArchiveExporter::zip(output_dir)?))
    }

    pub fn into_collect_exporter(self) -> Result<ArchiveExporter> {
        match self {
            Self::Archive(exporter) => Ok(exporter),
            Self::Directory(exporter) => Ok(ArchiveExporter::Directory(exporter)),
            unsupported => Err(eyre!(
                "Collect supports only directory or archive exporters, got {}",
                unsupported
            )),
        }
    }

    /// Consume a channel of documents and export them in batches with parallelism.
    ///
    /// This helper continuously receives documents from the provided
    /// `tokio::sync::mpsc::Receiver`, accumulating them until `batch_size`
    /// is reached, then sending the batch via the underlying exporter
    /// implementation (`Elasticsearch`, `File`, or `Stream`) for parallel processing.
    ///
    /// - A new `ProcessorSummary` is created for the provided `index`.
    /// - Documents are buffered up to `batch_size`; when the threshold is met
    ///   `send` is invoked for parallel processing and the accumulator is cleared.
    /// - When the sending side of the channel closes, any remaining (partial)
    ///   batch is also sent.
    /// - Batch responses are collected from parallel workers and merged into the summary.
    /// - Errors from batch processing do not abort processing; they are logged
    ///   with `log::warn!` and the loop continues.
    /// - The final (possibly partially updated) `ProcessorSummary` is returned.
    pub async fn document_channel<T: Serialize + Send + Sync + 'static>(
        self,
        mut rx: mpsc::Receiver<T>,
        index: String,
        batch_size: usize,
    ) -> ProcessorSummary {
        let mut summary = ProcessorSummary::new(index.clone());
        let mut accumulator = Vec::<T>::with_capacity(batch_size);
        let mut batch_receivers = Vec::new();
        while let Some(doc) = rx.recv().await {
            accumulator.push(doc);

            if accumulator.len() >= batch_size {
                let batch = std::mem::replace(&mut accumulator, Vec::with_capacity(batch_size));
                match self.tx(index.clone(), batch).await {
                    Ok(batch_rx) => batch_receivers.push(batch_rx),
                    Err(err) => log::warn!("Failed to send document batch: {}", err),
                }
            }
        }

        // Send final partial batch
        if !accumulator.is_empty() {
            match self.tx(index.clone(), accumulator).await {
                Ok(batch_rx) => batch_receivers.push(batch_rx),
                Err(err) => log::warn!("Failed to send final document batch: {}", err),
            }
        }

        // Collect all batch responses
        for batch_rx in batch_receivers {
            match batch_rx.await {
                Ok(batch_response) => summary.add_batch(batch_response),
                Err(_) => log::warn!("Batch response channel closed unexpectedly"),
            }
        }

        log::debug!("document_channel {} sent: {}", index, summary.docs);
        summary
    }

    pub async fn send<T>(
        &self,
        index: String,
        docs: Vec<T>,
    ) -> Result<crate::processor::BatchResponse>
    where
        T: Serialize + Sized + Send + Sync,
    {
        match self {
            Exporter::Archive(_) => Err(eyre!("batch send not supported for archive exporter")),
            Exporter::Directory(exporter) => exporter.batch_send(index, docs).await,
            Exporter::Elasticsearch(exporter) => exporter.batch_send(index, docs).await,
            Exporter::File(exporter) => exporter.batch_send(index, docs).await,
            Exporter::Stream(exporter) => exporter.batch_send(index, docs).await,
        }
    }

    pub fn get_docs_rx(&mut self) -> mpsc::Receiver<usize> {
        match self {
            Exporter::Archive(_) => {
                let (_tx, rx) = mpsc::channel::<usize>(1);
                rx
            }
            Exporter::Directory(exporter) => exporter.get_docs_rx(),
            Exporter::Elasticsearch(exporter) => exporter.get_docs_rx(),
            Exporter::File(exporter) => exporter.get_docs_rx(),
            Exporter::Stream(exporter) => exporter.get_docs_rx(),
        }
    }

    pub async fn tx<T>(
        &self,
        index: String,
        docs: Vec<T>,
    ) -> Result<oneshot::Receiver<crate::processor::BatchResponse>>
    where
        T: Serialize + Sized + Send + Sync + 'static,
    {
        match self {
            Exporter::Archive(_) => Err(eyre!("batch tx not supported for archive exporter")),
            Exporter::Directory(exporter) => exporter.batch_tx(index, docs).await,
            Exporter::Elasticsearch(exporter) => exporter.batch_tx(index, docs).await,
            Exporter::File(exporter) => exporter.batch_tx(index, docs).await,
            Exporter::Stream(exporter) => exporter.batch_tx(index, docs).await,
        }
    }

    pub async fn request(
        &self,
        method: &str,
        path: &str,
        value: Option<&serde_json::Value>,
    ) -> Result<::elasticsearch::http::response::Response> {
        match self {
            Exporter::Archive(_) => Err(eyre!("request not supported for archive exporter")),
            Exporter::Directory(_) => Err(eyre!("request not supported for directory exporter")),
            Exporter::Elasticsearch(exporter) => exporter.request(method, path, value).await,
            Exporter::File(_) => Err(eyre!("request not supported for file exporter")),
            Exporter::Stream(_) => Err(eyre!("request not supported for stream exporter")),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Exporter::Archive(_) => "archive",
            Exporter::Directory(_) => "directory",
            Exporter::Elasticsearch(_) => "elasticsearch",
            Exporter::File(_) => "file",
            Exporter::Stream(_) => "stream",
        }
    }

    pub fn cloned(&self) -> Self {
        self.clone()
    }

    pub fn target_value(&self) -> String {
        match self {
            Exporter::Archive(exporter) => exporter.to_string(),
            Exporter::Directory(exporter) => exporter.to_string(),
            Exporter::Elasticsearch(exporter) => exporter.to_string(),
            Exporter::File(exporter) => exporter.to_string(),
            Exporter::Stream(_) => "-".to_string(),
        }
    }

    pub fn target_label(&self) -> String {
        match self {
            Exporter::Archive(exporter) => format!("archive: {}", exporter),
            Exporter::Directory(exporter) => {
                format!("dir: {}", format_directory_label(&exporter.to_string()))
            }
            Exporter::Elasticsearch(exporter) => format!("elasticsearch: {}", exporter),
            Exporter::File(exporter) => format!("file: {}", exporter),
            Exporter::Stream(_) => "stdout: -".to_string(),
        }
    }

    pub async fn save_report(&self, report: &DiagnosticReport) -> Result<()> {
        match self {
            Exporter::Archive(_) => Err(eyre!("save report not supported for archive exporter")),
            Exporter::Directory(exporter) => exporter.save_report(report).await,
            Exporter::Elasticsearch(exporter) => exporter.save_report(report).await,
            Exporter::File(exporter) => exporter.save_report(report).await,
            Exporter::Stream(exporter) => exporter.save_report(report).await,
        }
    }

    pub async fn is_connected(&self) -> bool {
        match self {
            Exporter::Archive(exporter) => exporter.is_connected(),
            Exporter::Directory(exporter) => exporter.is_connected().await,
            Exporter::Elasticsearch(exporter) => exporter.is_connected().await,
            Exporter::File(exporter) => exporter.is_connected().await,
            Exporter::Stream(exporter) => exporter.is_connected().await,
        }
    }
}

impl Default for Exporter {
    fn default() -> Self {
        Exporter::Stream(StreamExporter::new())
    }
}

impl TryFrom<Uri> for Exporter {
    type Error = eyre::Report;
    fn try_from(uri: Uri) -> std::result::Result<Self, Self::Error> {
        match uri {
            Uri::Directory(dir) => Ok(Exporter::Directory(DirectoryExporter::try_from(dir)?)),
            Uri::File(file) => Ok(Exporter::File(FileExporter::try_from(file)?)),
            Uri::KnownHost(host) => Ok(Exporter::Elasticsearch(ElasticsearchExporter::try_from(
                host,
            )?)),
            Uri::Stream => Ok(Exporter::Stream(StreamExporter::new())),
            _ => Err(eyre!("Unsupported URI")),
        }
    }
}

impl std::fmt::Display for Exporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Exporter::Archive(exporter) => write!(f, "Archive {}", exporter),
            Exporter::Directory(exporter) => write!(f, "Directory {}", exporter),
            Exporter::Elasticsearch(exporter) => write!(f, "Elasticsearch {}", exporter),
            Exporter::File(exporter) => write!(f, "File {}", exporter),
            Exporter::Stream(exporter) => write!(f, "Stream {}", exporter),
        }
    }
}

fn format_directory_label(value: &str) -> String {
    if value.ends_with('/') || value.ends_with('\\') {
        value.to_string()
    } else {
        format!("{value}{}", std::path::MAIN_SEPARATOR)
    }
}

impl TryFrom<KnownHost> for Exporter {
    type Error = eyre::Report;
    fn try_from(host: KnownHost) -> std::result::Result<Self, Self::Error> {
        match host.app() {
            Product::Elasticsearch => Ok(Exporter::Elasticsearch(ElasticsearchExporter::try_from(
                host,
            )?)),
            _ => Err(eyre!("Unsupported product")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::format_directory_label;

    #[test]
    fn format_directory_label_preserves_existing_trailing_separator() {
        assert_eq!(format_directory_label("/tmp/out/"), "/tmp/out/");
        assert_eq!(format_directory_label(r"C:\out\"), r"C:\out\");
    }

    #[test]
    fn format_directory_label_appends_platform_separator_when_missing() {
        assert_eq!(
            format_directory_label("/tmp/out"),
            format!("/tmp/out{}", std::path::MAIN_SEPARATOR)
        );
    }
}
