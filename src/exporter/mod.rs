// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Write to a directory
mod directory;
/// Export to an Elasticsearch cluster with the `_bulk` API
mod elasticsearch;
/// Write to an `.ndjson` file
mod file;
/// Write `ndjson` to std out
mod stream;

use crate::{
    client::{KnownHost, KnownHostBuilder},
    data::Uri,
    processor::{DiagnosticReport, Identifiers, ProcessorSummary, Product},
};
pub use directory::DirectoryExporter;
use elasticsearch::ElasticsearchExporter;
use eyre::{Result, eyre};
use file::FileExporter;
use serde::Serialize;
use stream::StreamExporter;
use tokio::sync::mpsc;
use url::Url;

trait Export {
    async fn is_connected(&self) -> bool;
    async fn write<T>(&self, summary: &mut ProcessorSummary, docs: &mut Vec<T>) -> Result<()>
    where
        T: Serialize + Sized + Send + Sync;
    async fn save_report(&self, report: &DiagnosticReport) -> Result<()>;
    fn with_identifiers(self, identifiers: Identifiers) -> Self;
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
    /// Export to an Elasticsearch cluster with the `_bulk` API
    Elasticsearch(ElasticsearchExporter),
    /// Export to an `.ndjson` file
    File(FileExporter),
    /// Export to `stdout`
    Stream(StreamExporter),
}

impl Exporter {
    /// Consume a channel of documents and export them in batches.
    ///
    /// This helper continuously receives documents from the provided
    /// `tokio::sync::mpsc::Receiver`, accumulating them until `batch_size`
    /// is reached, then flushing the batch via the underlying exporter
    /// implementation (`Elasticsearch`, `File`, or `Stream`).
    ///
    /// - A new `ProcessorSummary` is created for the provided `index`.
    /// - Documents are buffered up to `batch_size`; when the threshold is met
    ///   `write` is invoked and the accumulator is cleared.
    /// - When the sending side of the channel closes, any remaining (partial)
    ///   batch is also written.
    /// - Errors from batch writes do not abort processing; they are logged
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

        while let Some(doc) = rx.recv().await {
            accumulator.push(doc);

            if accumulator.len() >= batch_size {
                if let Err(err) = self.write(&mut summary, &mut accumulator).await {
                    log::warn!("Failed to write document batch: {}", err);
                }
                accumulator.clear();
            }
        }

        // Write final partial batch
        if !accumulator.is_empty() {
            if let Err(err) = self.write(&mut summary, &mut accumulator).await {
                log::warn!("Failed to write final index documents batch: {}", err);
            }
        }

        log::debug!("document_channel {} sent: {}", index, summary.docs);
        summary
    }

    pub async fn write<T>(&self, summary: &mut ProcessorSummary, docs: &mut Vec<T>) -> Result<()>
    where
        T: Serialize + Sized + Send + Sync,
    {
        match self {
            Exporter::Elasticsearch(exporter) => exporter.write(summary, docs).await,
            Exporter::File(exporter) => exporter.write(summary, docs).await,
            Exporter::Stream(exporter) => exporter.write(summary, docs).await,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Exporter::Elasticsearch(_) => "elasticsearch",
            Exporter::File(_) => "file",
            Exporter::Stream(_) => "stream",
        }
    }

    pub fn cloned(&self) -> Self {
        self.clone()
    }

    pub async fn save_report(&self, report: &DiagnosticReport) -> Result<()> {
        match self {
            Exporter::Elasticsearch(exporter) => exporter.save_report(report).await,
            Exporter::File(exporter) => exporter.save_report(report).await,
            Exporter::Stream(exporter) => exporter.save_report(report).await,
        }
    }

    pub async fn is_connected(&self) -> bool {
        match self {
            Exporter::Elasticsearch(exporter) => exporter.is_connected().await,
            Exporter::File(exporter) => exporter.is_connected().await,
            Exporter::Stream(exporter) => exporter.is_connected().await,
        }
    }

    pub fn with_identifiers(self, identifiers: Identifiers) -> Self {
        match self {
            Exporter::Elasticsearch(exporter) => {
                Exporter::Elasticsearch(exporter.with_identifiers(identifiers))
            }
            Exporter::File(exporter) => Exporter::File(exporter.with_identifiers(identifiers)),
            Exporter::Stream(exporter) => Exporter::Stream(exporter.with_identifiers(identifiers)),
        }
    }

    pub fn identifiers(&self) -> Identifiers {
        match self {
            Exporter::Elasticsearch(exporter) => exporter.identifiers.clone(),
            Exporter::File(exporter) => exporter.identifiers.clone(),
            Exporter::Stream(exporter) => exporter.identifiers.clone(),
        }
    }
}

impl Default for Exporter {
    fn default() -> Self {
        Exporter::Stream(StreamExporter::new())
    }
}

impl TryFrom<Option<Uri>> for Exporter {
    type Error = eyre::Report;
    fn try_from(uri: Option<Uri>) -> std::result::Result<Self, Self::Error> {
        if let Some(uri) = uri {
            match uri {
                Uri::File(file) => Ok(Exporter::File(FileExporter::try_from(file)?)),
                Uri::KnownHost(host) => Ok(Exporter::Elasticsearch(
                    ElasticsearchExporter::try_from(host)?,
                )),
                Uri::Stream => Ok(Exporter::Stream(StreamExporter::new())),
                _ => Err(eyre!("Unsupported URI")),
            }
        } else {
            log::debug!("No output given, using ESDIAG_OUTPUT_URL");
            let output_url = std::env::var("ESDIAG_OUTPUT_URL")
                .map_err(|_| eyre!("ESDIAG_OUTPUT_URL is not defined"))?;
            log::info!("output: Env {}", output_url);
            let apikey = std::env::var("ESDIAG_OUTPUT_APIKEY").ok();
            let username = std::env::var("ESDIAG_OUTPUT_USERNAME").ok();
            let password = std::env::var("ESDIAG_OUTPUT_PASSWORD").ok();
            let host = KnownHostBuilder::new(Url::parse(&output_url)?)
                .apikey(apikey)
                .username(username)
                .password(password)
                .build()?;
            Ok(Exporter::Elasticsearch(ElasticsearchExporter::try_from(
                host,
            )?))
        }
    }
}

impl std::fmt::Display for Exporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Exporter::Elasticsearch(exporter) => write!(f, "Elasticsearch {}", exporter),
            Exporter::File(exporter) => write!(f, "File {}", exporter),
            Exporter::Stream(exporter) => write!(f, "Stream {}", exporter),
        }
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
