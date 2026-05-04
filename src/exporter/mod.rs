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
use std::path::{Path, PathBuf};
use stream::StreamExporter;
use tokio::sync::{mpsc, oneshot};
use url::Url;

trait Export {
    async fn is_connected(&self) -> bool;
    async fn batch_send<T>(&self, index: String, docs: Vec<T>) -> Result<BatchResponse>
    where
        T: Serialize + Sized + Send + Sync;
    async fn batch_tx<T>(&self, index: String, docs: Vec<T>) -> Result<oneshot::Receiver<BatchResponse>>
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
    /// Export collected bundles to a directory or zip archive
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
            Uri::Directory(path) | Uri::File(path) => Ok(Self::Directory(DirectoryExporter::try_from(path)?)),
            _ => Err(eyre!("Collect requires a local directory output when --zip is not set")),
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
    ///   with `tracing::warn!` and the loop continues.
    /// - The final (possibly partially updated) `ProcessorSummary` is returned.
    #[tracing::instrument(skip_all, fields(index = %index))]
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
                    Err(err) => tracing::warn!("Failed to send document batch: {}", err),
                }
            }
        }

        // Send final partial batch
        if !accumulator.is_empty() {
            match self.tx(index.clone(), accumulator).await {
                Ok(batch_rx) => batch_receivers.push(batch_rx),
                Err(err) => tracing::warn!("Failed to send final document batch: {}", err),
            }
        }

        // Collect all batch responses
        for batch_rx in batch_receivers {
            match batch_rx.await {
                Ok(batch_response) => summary.add_batch(batch_response),
                Err(_) => tracing::warn!("Batch response channel closed unexpectedly"),
            }
        }

        tracing::debug!("document_channel {} sent: {}", index, summary.docs);
        summary
    }

    #[tracing::instrument(skip_all, fields(index = %index))]
    pub async fn send<T>(&self, index: String, docs: Vec<T>) -> Result<crate::processor::BatchResponse>
    where
        T: Serialize + Sized + Send + Sync,
    {
        if docs.is_empty() {
            return Ok(crate::processor::BatchResponse {
                docs: 0,
                errors: 0,
                retries: 0,
                size: 0,
                status_code: 200,
                time: 0,
            });
        }

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

    pub async fn tx<T>(&self, index: String, docs: Vec<T>) -> Result<oneshot::Receiver<crate::processor::BatchResponse>>
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

    pub fn target_uri(&self) -> String {
        match self {
            Exporter::Archive(exporter) => path_to_file_uri(Path::new(&exporter.to_string()), false),
            Exporter::Directory(exporter) => path_to_file_uri(Path::new(&exporter.to_string()), true),
            Exporter::Elasticsearch(exporter) => exporter.to_string(),
            Exporter::File(exporter) => path_to_file_uri(Path::new(&exporter.to_string()), false),
            Exporter::Stream(_) => "stdio://stdout".to_string(),
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

    pub fn requires_secret(&self) -> bool {
        match self {
            Exporter::Elasticsearch(exporter) => exporter.requires_secret(),
            Exporter::Archive(_) | Exporter::Directory(_) | Exporter::File(_) | Exporter::Stream(_) => false,
        }
    }

    pub fn kibana_base_url(&self) -> Option<String> {
        match self {
            Exporter::Elasticsearch(exporter) => exporter.kibana_base_url(),
            Exporter::Archive(_) | Exporter::Directory(_) | Exporter::File(_) | Exporter::Stream(_) => {
                kibana_base_url_from_env()
            }
        }
    }

    pub fn kibana_link(&self, diagnostic_id: &str, collection_date: u64) -> Option<String> {
        self.kibana_base_url()
            .map(|kibana_url| build_kibana_link(&kibana_url, diagnostic_id, collection_date))
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
            Uri::KnownHost(host) => Ok(Exporter::Elasticsearch(ElasticsearchExporter::try_from(host)?)),
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

fn path_to_file_uri(path: &Path, is_dir: bool) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let url = if is_dir {
        Url::from_directory_path(&absolute)
    } else {
        Url::from_file_path(&absolute)
    };
    url.map(|url| url.to_string())
        .unwrap_or_else(|_| absolute.display().to_string())
}

impl TryFrom<KnownHost> for Exporter {
    type Error = eyre::Report;
    fn try_from(host: KnownHost) -> std::result::Result<Self, Self::Error> {
        match host.app() {
            Product::Elasticsearch => Ok(Exporter::Elasticsearch(ElasticsearchExporter::try_from(host)?)),
            _ => Err(eyre!("Unsupported product")),
        }
    }
}

fn saved_viewer_kibana_base_url(host: &KnownHost) -> Option<String> {
    let viewer_name = host.viewer()?;
    let viewer_name = viewer_name.to_string();
    let viewer_host = match KnownHost::get_known(&viewer_name) {
        Some(viewer_host) => viewer_host,
        None => {
            tracing::warn!(
                "Output host viewer '{}' was not found at runtime; falling back to environment Kibana URL",
                viewer_name
            );
            return None;
        }
    };

    if !viewer_host.has_role(crate::data::HostRole::View) || viewer_host.app() != &Product::Kibana {
        tracing::warn!(
            "Output host viewer '{}' is not a valid Kibana view target; falling back to environment Kibana URL",
            viewer_name
        );
        return None;
    }

    viewer_host
        .concrete_url()
        .map(|url| crate::env::append_kibana_space(url.as_ref()))
}

fn kibana_base_url_from_env() -> Option<String> {
    crate::env::get_string("ESDIAG_KIBANA_URL")
        .ok()
        .map(|url| crate::env::append_kibana_space(&url))
}

fn build_kibana_link(kibana_url: &str, diagnostic_id: &str, collection_date: u64) -> String {
    let url_safe_id = urlencoding::encode(diagnostic_id);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let days_since_collection = now_ms.saturating_sub(collection_date) / (1000 * 60 * 60 * 24);
    let time_filter = match days_since_collection {
        x if x < 90 => "from:now-90d,to:now".to_string(),
        x if (90..365).contains(&x) => "from:now-1y,to:now".to_string(),
        x => format!("from:now-{}d,to:now", x + 1),
    };
    format!(
        "{}/app/dashboards#/view/elasticsearch-cluster-report?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:'4319ebc4-df81-4b18-b8bd-6aaa55a1fd13',key:diagnostic.id,negate:!f,params:(query:'{}'),type:phrase),query:(match_phrase:(diagnostic.id:'{}')))),refreshInterval:(pause:!t,value:60000),time:({}))",
        kibana_url, url_safe_id, url_safe_id, time_filter
    )
}

#[cfg(test)]
mod tests {
    use super::{ArchiveExporter, Exporter, format_directory_label};
    use crate::data::{HostRole, KnownHost, KnownHostBuilder, Product, Uri};
    use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};
    use tempfile::TempDir;
    use url::Url;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_env_lock()
    }

    fn setup_env() -> TempDir {
        let tmp = TempDir::new().expect("temp dir");
        let hosts = tmp.path().join("hosts.yml");
        let keystore = tmp.path().join("secrets.yml");
        unsafe {
            std::env::set_var("ESDIAG_HOSTS", &hosts);
            std::env::set_var("ESDIAG_KEYSTORE", &keystore);
        }
        tmp
    }

    #[test]
    fn format_directory_label_preserves_existing_trailing_separator() {
        assert_eq!(format_directory_label("/tmp/out/"), "/tmp/out/");
        assert_eq!(format_directory_label(r"C:\out\"), r"C:\out\");
    }

    #[test]
    fn send_empty_batch_succeeds_without_backend_support() {
        let tmp = TempDir::new().expect("temp dir");
        let exporter = Exporter::Archive(ArchiveExporter::zip(tmp.path().to_path_buf()).expect("archive exporter"));

        let response =
            tokio_test::block_on(exporter.send::<serde_json::Value>("settings-slm-esdiag".to_string(), Vec::new()))
                .expect("empty send response");

        assert_eq!(response.docs, 0);
        assert_eq!(response.errors, 0);
        assert_eq!(response.status_code, 200);
    }

    #[test]
    fn format_directory_label_appends_platform_separator_when_missing() {
        assert_eq!(
            format_directory_label("/tmp/out"),
            format!("/tmp/out{}", std::path::MAIN_SEPARATOR)
        );
    }

    #[test]
    fn target_uri_uses_canonical_machine_values() {
        let directory = Exporter::try_from(Uri::Directory(PathBuf::from("/tmp/out"))).expect("directory exporter");
        assert_eq!(directory.target_uri(), "file:///tmp/out/");
        assert_eq!(directory.target_label(), "dir: /tmp/out/");

        let file = Exporter::try_from(Uri::File(PathBuf::from("/tmp/out/report.ndjson"))).expect("file exporter");
        assert_eq!(file.target_uri(), "file:///tmp/out/report.ndjson");
        assert_eq!(file.target_label(), "file: /tmp/out/report.ndjson");

        let stream = Exporter::default();
        assert_eq!(stream.target_uri(), "stdio://stdout");
        assert_eq!(stream.target_label(), "stdout: -");
    }

    #[test]
    fn kibana_link_prefers_saved_viewer_host() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "send-host".to_string(),
            KnownHostBuilder::new(Url::parse("https://es.example:9200").expect("es url"))
                .product(Product::Elasticsearch)
                .roles(vec![HostRole::Send])
                .viewer(Some("viewer-host".to_string()))
                .build()
                .expect("send host"),
        );
        hosts.insert(
            "viewer-host".to_string(),
            KnownHostBuilder::new(Url::parse("https://kb.example:5601").expect("kb url"))
                .product(Product::Kibana)
                .roles(vec![HostRole::View])
                .build()
                .expect("viewer host"),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");
        unsafe {
            std::env::set_var("ESDIAG_KIBANA_URL", "https://env-kb.example:5601");
            std::env::remove_var("ESDIAG_KIBANA_SPACE");
        }

        let exporter = Exporter::try_from(Uri::try_from("send-host").expect("host uri")).expect("exporter");
        let kibana_link = exporter
            .kibana_link("diag-123", 1_700_000_000_000)
            .expect("kibana link");

        assert!(kibana_link.starts_with("https://kb.example:5601/s/esdiag/app/dashboards#/view/"));

        unsafe {
            std::env::remove_var("ESDIAG_KIBANA_URL");
        }
    }

    #[test]
    fn kibana_link_falls_back_to_env_for_non_host_outputs() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        unsafe {
            std::env::set_var("ESDIAG_KIBANA_URL", "https://env-kb.example:5601");
            std::env::set_var("ESDIAG_KIBANA_SPACE", "ops");
        }

        let exporter = Exporter::default();
        let kibana_link = exporter
            .kibana_link("diag-123", 1_700_000_000_000)
            .expect("kibana link");

        assert!(kibana_link.starts_with("https://env-kb.example:5601/s/ops/app/dashboards#/view/"));

        unsafe {
            std::env::remove_var("ESDIAG_KIBANA_URL");
            std::env::remove_var("ESDIAG_KIBANA_SPACE");
        }
    }

    #[test]
    fn kibana_link_omits_space_when_explicitly_empty() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        unsafe {
            std::env::set_var("ESDIAG_KIBANA_URL", "https://env-kb.example:5601");
            std::env::set_var("ESDIAG_KIBANA_SPACE", "");
        }

        let exporter = Exporter::default();
        let kibana_link = exporter
            .kibana_link("diag-123", 1_700_000_000_000)
            .expect("kibana link");

        assert!(kibana_link.starts_with("https://env-kb.example:5601/app/dashboards#/view/"));

        unsafe {
            std::env::remove_var("ESDIAG_KIBANA_URL");
            std::env::remove_var("ESDIAG_KIBANA_SPACE");
        }
    }

    #[test]
    fn kibana_link_falls_back_to_default_kibana_url_when_no_override_exists() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        unsafe {
            std::env::remove_var("ESDIAG_KIBANA_URL");
            std::env::remove_var("ESDIAG_KIBANA_SPACE");
        }

        let exporter = Exporter::default();

        let kibana_link = exporter
            .kibana_link("diag-123", 1_700_000_000_000)
            .expect("default kibana link");

        assert!(kibana_link.starts_with("http://localhost:5601/s/esdiag/app/dashboards#/view/"));
    }
}
