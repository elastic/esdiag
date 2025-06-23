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
    data::{
        Uri,
        diagnostic::{DiagnosticReport, Product, report::ProcessorSummary},
    },
};
pub use directory::DirectoryExporter;
use elasticsearch::ElasticsearchExporter;
use eyre::{Result, eyre};
use file::FileExporter;
use serde_json::Value;
use stream::StreamExporter;
use url::Url;

trait Export {
    async fn is_connected(&self) -> bool;
    async fn write(&self, index: String, docs: Vec<Value>) -> Result<ProcessorSummary>;
    async fn save_report(&self, report: &DiagnosticReport) -> Result<()>;
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
    pub async fn write(&self, index: String, docs: Vec<Value>) -> Result<ProcessorSummary> {
        match self {
            Exporter::Elasticsearch(exporter) => exporter.write(index, docs).await,
            Exporter::File(exporter) => exporter.write(index, docs).await,
            Exporter::Stream(exporter) => exporter.write(index, docs).await,
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
