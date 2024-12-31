/// Write to a directory
mod directory;
/// Export to an Elasticsearch cluster with the `_bulk` API
mod elasticsearch;
/// Write to an `.ndjson` file
mod file;
/// Write `ndjson` to std out
mod stream;

pub use directory::DirectoryExporter;
use elasticsearch::ElasticsearchExporter;
use file::FileExporter;
use stream::StreamExporter;

use crate::data::Uri;
use color_eyre::eyre::{eyre, Result};
use serde_json::Value;

trait Export {
    #[allow(dead_code)]
    async fn is_connected(&self) -> bool;
    async fn write(&self, index: String, docs: Vec<Value>) -> Result<usize>;
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
    pub async fn write(&self, index: String, docs: Vec<Value>) -> Result<usize> {
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
}

impl TryFrom<Uri> for Exporter {
    type Error = color_eyre::Report;
    fn try_from(uri: Uri) -> std::result::Result<Self, Self::Error> {
        match uri {
            Uri::File(file) => Ok(Exporter::File(FileExporter::try_from(file)?)),
            Uri::KnownHost(host) => Ok(Exporter::Elasticsearch(ElasticsearchExporter::try_from(
                host,
            )?)),
            Uri::Stream => Ok(Exporter::Stream(StreamExporter::new())),
            _ => Err(eyre!("Unsupported URI")),
        }
    }
}
