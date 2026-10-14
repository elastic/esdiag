// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Write collection output to a zip archive
mod archive;
/// Role-typed raw bundle output used by the Save stage.
mod bundle;
/// Write collection output to a directory
mod directory;
/// Role-typed processed document output used by the Export stage.
mod document;
/// Send to an Elasticsearch cluster with the `_bulk` API
mod elasticsearch;
/// Write to an `.ndjson` file
mod file;
/// Write `ndjson` to std out
mod stream;

use crate::{
    data::{Application, KnownHost, Platform, Uri},
    processor::{BatchResponse, DiagnosticReport, ProcessorSummary},
};
pub use archive::ArchiveExporter;
pub use bundle::BundleExporter;
pub use directory::DirectoryExporter;
pub use document::DocumentExporter;
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
                let doc_count = batch.len() as u32;
                match self.tx(index.clone(), batch).await {
                    Ok(batch_rx) => batch_receivers.push(batch_rx),
                    Err(err) => {
                        tracing::warn!("Failed to send document batch: {}", err);
                        summary.add_batch(BatchResponse::failed(doc_count, self.failed_request_status()));
                    }
                }
            }
        }

        // Send final partial batch
        if !accumulator.is_empty() {
            let doc_count = accumulator.len() as u32;
            match self.tx(index.clone(), accumulator).await {
                Ok(batch_rx) => batch_receivers.push(batch_rx),
                Err(err) => {
                    tracing::warn!("Failed to send final document batch: {}", err);
                    summary.add_batch(BatchResponse::failed(doc_count, self.failed_request_status()));
                }
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
            let mut response = crate::processor::BatchResponse::aggregate();
            response.status_code = self.no_request_status();
            return Ok(response);
        }
        if matches!(self, Exporter::Archive(_)) {
            return Err(eyre!("batch send not supported for archive exporter"));
        }

        self.send_batch_with_failed_response(index, docs).await
    }

    async fn send_batch_with_failed_response<T>(
        &self,
        index: String,
        docs: Vec<T>,
    ) -> Result<crate::processor::BatchResponse>
    where
        T: Serialize + Sized + Send + Sync,
    {
        let doc_count = docs.len() as u32;
        match self.send_batch(index, docs).await {
            Ok(response) => Ok(response),
            Err(err) => {
                tracing::warn!("Failed to send document batch: {}", err);
                Ok(BatchResponse::failed(doc_count, self.failed_request_status()))
            }
        }
    }

    /// The request status recorded when this exporter fails without a
    /// response: HTTP exporters use the 599 sentinel; status `0` is reserved
    /// for non-HTTP exporters (file, stream, directory) — ADR-0016.
    fn failed_request_status(&self) -> u16 {
        match self {
            Exporter::Elasticsearch(_) => BatchResponse::HTTP_REQUEST_NOT_COMPLETED,
            _ => 0,
        }
    }

    /// The request status recorded when a result is produced without an HTTP
    /// round trip, such as an empty batch. A non-HTTP exporter keeps the
    /// reserved `0`; the Elasticsearch exporter never reports `0`, because
    /// that code means "no HTTP transport" rather than "no request needed"
    /// (ADR-0016).
    fn no_request_status(&self) -> u16 {
        match self {
            Exporter::Elasticsearch(_) => 200,
            _ => 0,
        }
    }

    async fn send_batch<T>(&self, index: String, docs: Vec<T>) -> Result<crate::processor::BatchResponse>
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

    /// The user-facing output URI reported after processing completes.
    ///
    /// Prefer the configured Kibana base URL for Elasticsearch output so the
    /// result identifies where the exported documents can be viewed. Other
    /// exporters report their canonical destination URI.
    pub fn outcome_uri(&self) -> String {
        self.kibana_base_url().unwrap_or_else(|| self.target_uri())
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
            Exporter::Archive(_) | Exporter::Directory(_) | Exporter::File(_) | Exporter::Stream(_) => None,
        }
    }

    pub fn kibana_link(&self, target: KibanaLinkTarget, diagnostic_id: &str, collection_date: u64) -> Option<String> {
        self.kibana_base_url().map(|kibana_url| {
            build_kibana_link(
                &kibana_url,
                diagnostic_id,
                collection_date,
                target.dashboard_id(),
                target.data_view_id(),
            )
        })
    }

    pub async fn resolved_kibana_link(
        &self,
        target: KibanaLinkTarget,
        diagnostic_id: &str,
        collection_date: u64,
    ) -> Option<String> {
        let Self::Elasticsearch(exporter) = self else {
            return None;
        };
        let base = exporter.kibana_base_url()?;
        let viewer = *exporter.kibana_viewer.clone()?;
        match resolve_kibana_link_ids(viewer, &base, target).await {
            Ok((dashboard, data_view)) => Some(build_kibana_link(
                &base,
                diagnostic_id,
                collection_date,
                &dashboard,
                &data_view,
            )),
            Err(error) => {
                tracing::warn!("Unable to resolve the output's Kibana dashboard; omitting its link: {error}");
                None
            }
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
            Some(Application::Elasticsearch) => Ok(Exporter::Elasticsearch(ElasticsearchExporter::try_from(host)?)),
            _ => Err(eyre!("Export requires an Elasticsearch application host")),
        }
    }
}

fn saved_viewer(host: &KnownHost) -> Option<KnownHost> {
    let viewer_name = host.viewer()?;
    let viewer_name = viewer_name.to_string();
    let viewer_host = match KnownHost::get_known(&viewer_name) {
        Some(viewer_host) => viewer_host,
        None => {
            tracing::warn!(
                "Output host viewer '{}' was not found at runtime; omitting its Kibana link",
                viewer_name
            );
            return None;
        }
    };

    if !viewer_host.has_role(crate::data::HostRole::View) || viewer_host.app() != Some(Application::Kibana) {
        tracing::warn!(
            "Output host viewer '{}' is not a valid Kibana view target; omitting its Kibana link",
            viewer_name
        );
        return None;
    }

    Some(viewer_host)
}

fn kibana_base_url_from_env() -> Option<String> {
    let url = std::env::var("ESDIAG_KIBANA_URL").ok()?;
    let url = Url::parse(url.trim()).ok()?;
    Some(kibana_link_base(&url))
}

/// The Kibana base for links and outcome URIs. URL userinfo is dropped so a
/// credential embedded in the viewer URL never reaches reports or CLI results;
/// requests still authenticate through the viewer host.
fn kibana_link_base(url: &Url) -> String {
    let mut url = url.clone();
    let _ = url.set_username("");
    let _ = url.set_password(None);
    crate::env::append_kibana_space(url.as_str())
}

/// The Kibana host that views `host`'s data: the one resolution attached, or
/// the saved viewer `host` names in `hosts.yml`.
fn output_viewer(host: &KnownHost) -> Option<KnownHost> {
    if let Some(viewer) = host.attached_viewer() {
        return Some(viewer.clone());
    }
    host.viewer()?;
    saved_viewer(host)
}

fn kibana_base_url_for_output(viewer: &KnownHost) -> Option<String> {
    viewer.concrete_url().map(kibana_link_base)
}

/// The dashboard a diagnostic's Kibana link opens, with the data view its
/// `diagnostic.id` filter is bound to. Ids are the shipped saved-object ids,
/// matched against `originId` after Kibana migrations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KibanaLinkTarget {
    ClusterReport,
    EckPlatform,
}

impl KibanaLinkTarget {
    /// ECK parent bundles (platform-only) open the ECK dashboard; every
    /// application diagnostic, including ECK children, opens the Cluster Report.
    pub fn for_diagnostic(platform: Platform, application: Option<Application>) -> Self {
        match (platform, application) {
            (Platform::ECK, None) => Self::EckPlatform,
            _ => Self::ClusterReport,
        }
    }

    fn dashboard_id(self) -> &'static str {
        match self {
            Self::ClusterReport => "elasticsearch-cluster-report",
            Self::EckPlatform => "eck-platform-overview",
        }
    }

    fn data_view_id(self) -> &'static str {
        match self {
            Self::ClusterReport => "4319ebc4-df81-4b18-b8bd-6aaa55a1fd13",
            Self::EckPlatform => "cab5c85e-e296-56a8-a8db-ae3af0d095f3",
        }
    }

    fn dashboard_name(self) -> &'static str {
        match self {
            Self::ClusterReport => "Cluster Report",
            Self::EckPlatform => "ECK Platform Overview",
        }
    }

    fn data_view_name(self) -> &'static str {
        match self {
            Self::ClusterReport => "Node Settings",
            Self::EckPlatform => "ECK Resources",
        }
    }
}

async fn resolve_kibana_link_ids(viewer: Uri, base: &str, target: KibanaLinkTarget) -> Result<(String, String)> {
    let client = crate::client::Client::try_from(viewer)?;
    let prefix = Url::parse(base)?.path().trim_end_matches('/').to_string();
    let mut dashboard = None;
    let mut data_view = None;
    let headers = std::collections::HashMap::from([("X-Elastic-Internal-Origin".to_string(), "Kibana".to_string())]);
    for page in 1.. {
        let response = client
            .request(
                reqwest::Method::GET,
                &headers,
                &format!("{prefix}/api/saved_objects/_find?type=dashboard&type=index-pattern&per_page=100&page={page}"),
                None,
            )
            .await?;
        eyre::ensure!(
            response.status().is_success(),
            "Kibana object lookup returned {}",
            response.status()
        );
        let value: serde_json::Value = response.json().await?;
        let objects = value["saved_objects"]
            .as_array()
            .ok_or_else(|| eyre!("Missing saved objects in Kibana response"))?;
        for object in objects {
            let original = object["originId"].as_str().or_else(|| object["id"].as_str());
            let id = object["id"].as_str().map(str::to_string);
            match (object["type"].as_str(), original) {
                (Some("dashboard"), Some(origin)) if origin == target.dashboard_id() => dashboard = id,
                (Some("index-pattern"), Some(origin)) if origin == target.data_view_id() => data_view = id,
                _ => {}
            }
        }
        if dashboard.is_some() && data_view.is_some() {
            break;
        }
        if objects.is_empty() || page * 100 >= value["total"].as_u64().unwrap_or(0) {
            break;
        }
    }
    Ok((
        dashboard.ok_or_else(|| {
            eyre!(
                "{} is not installed in the output's Kibana space",
                target.dashboard_name()
            )
        })?,
        data_view.ok_or_else(|| {
            eyre!(
                "{} data view is not installed in the output's Kibana space",
                target.data_view_name()
            )
        })?,
    ))
}

fn build_kibana_link(
    kibana_url: &str,
    diagnostic_id: &str,
    collection_date: u64,
    dashboard_id: &str,
    data_view_id: &str,
) -> String {
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
        "{}/app/dashboards#/view/{}?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:'{}',key:diagnostic.id,negate:!f,params:(query:'{}'),type:phrase),query:(match_phrase:(diagnostic.id:'{}')))),refreshInterval:(pause:!t,value:60000),time:({}))",
        kibana_url,
        urlencoding::encode(dashboard_id),
        urlencoding::encode(data_view_id),
        url_safe_id,
        url_safe_id,
        time_filter
    )
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn migrated_kibana_ids_are_resolved_in_the_selected_space_across_pages() {
        use axum::{Json, Router, routing::get};
        use serde_json::json;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/s/ops", listener.local_addr().unwrap());
        let router = Router::new().route("/s/ops/api/saved_objects/_find", get(|uri: axum::http::Uri| async move {
            assert!(uri.query().unwrap().contains("type=dashboard&type=index-pattern"));
                if uri.query().unwrap().ends_with("&page=1") {
                Json(json!({"total":101,"saved_objects":[{"type":"dashboard","id":"unrelated"}]}))
            } else {
                Json(json!({"total":101,"saved_objects":[
                    {"type":"dashboard","id":"migrated-dashboard","originId":"elasticsearch-cluster-report"},
                    {"type":"index-pattern","id":"migrated-data-view","originId":"4319ebc4-df81-4b18-b8bd-6aaa55a1fd13"}
                ]}))
            }
        }));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let viewer = KnownHostBuilder::new(Url::parse(&base).unwrap())
            .application(Application::Kibana)
            .build()
            .unwrap();
        let (dashboard, data_view) =
            super::resolve_kibana_link_ids(Uri::KnownHost(viewer), &base, super::KibanaLinkTarget::ClusterReport)
                .await
                .unwrap();
        let link = super::build_kibana_link(&base, "diag@today", 0, &dashboard, &data_view);
        assert!(link.contains("/s/ops/app/dashboards#/view/migrated-dashboard?"));
        assert!(link.contains("index:'migrated-data-view'"));
        assert!(link.contains("diag%40today"));
        server.abort();
    }

    #[test]
    fn eck_parents_link_to_the_eck_dashboard_and_children_to_the_cluster_report() {
        assert_eq!(
            KibanaLinkTarget::for_diagnostic(Platform::ECK, None),
            KibanaLinkTarget::EckPlatform
        );
        assert_eq!(
            KibanaLinkTarget::for_diagnostic(Platform::ECK, Some(Application::Elasticsearch)),
            KibanaLinkTarget::ClusterReport
        );
        assert_eq!(
            KibanaLinkTarget::for_diagnostic(Platform::SelfManaged, Some(Application::Elasticsearch)),
            KibanaLinkTarget::ClusterReport
        );
    }

    #[test]
    fn link_targets_name_shipped_saved_objects() {
        let objects = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/kibana/esdiag/objects");
        for target in [KibanaLinkTarget::ClusterReport, KibanaLinkTarget::EckPlatform] {
            for (dir, id) in [
                ("dashboard", target.dashboard_id()),
                ("index-pattern", target.data_view_id()),
            ] {
                let shipped = std::fs::read_dir(objects.join(dir))
                    .unwrap()
                    .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
                    .any(|contents| contents.contains(&format!("\"id\": \"{id}\"")));
                assert!(shipped, "{target:?} {dir} {id} is not a shipped saved object");
            }
        }
    }

    #[tokio::test]
    async fn eck_link_resolves_the_eck_dashboard_ids() {
        use axum::{Json, Router, routing::get};
        use serde_json::json;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let router = Router::new().route(
            "/api/saved_objects/_find",
            get(|| async {
                Json(json!({"total":4,"saved_objects":[
                    {"type":"dashboard","id":"elasticsearch-cluster-report"},
                    {"type":"index-pattern","id":"4319ebc4-df81-4b18-b8bd-6aaa55a1fd13"},
                    {"type":"dashboard","id":"eck-platform-overview"},
                    {"type":"index-pattern","id":"cab5c85e-e296-56a8-a8db-ae3af0d095f3"}
                ]}))
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let viewer = KnownHostBuilder::new(Url::parse(&base).unwrap())
            .application(Application::Kibana)
            .build()
            .unwrap();
        let (dashboard, data_view) =
            super::resolve_kibana_link_ids(Uri::KnownHost(viewer), &base, KibanaLinkTarget::EckPlatform)
                .await
                .unwrap();
        assert_eq!(dashboard, "eck-platform-overview");
        assert_eq!(data_view, "cab5c85e-e296-56a8-a8db-ae3af0d095f3");
        server.abort();
    }

    use super::{ArchiveExporter, Exporter, KibanaLinkTarget, format_directory_label};
    use crate::data::{Application, HostRole, KnownHost, KnownHostBuilder, Platform, Uri};
    use std::{collections::BTreeMap, path::PathBuf, time::Duration};
    use tempfile::TempDir;
    use url::Url;

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
        assert_eq!(response.batch_count, 0);
        // Reserved for a non-HTTP exporter: there was no request to report.
        assert_eq!(response.status_code, 0);
    }

    #[tokio::test]
    async fn non_http_exporters_report_the_reserved_request_status() {
        let tmp = TempDir::new().expect("temp dir");
        let exporters = [
            Exporter::try_from(Uri::File(tmp.path().join("documents.ndjson"))).expect("file exporter"),
            Exporter::try_from(Uri::Directory(tmp.path().to_path_buf())).expect("directory exporter"),
            Exporter::try_from(Uri::Stream).expect("stream exporter"),
        ];

        for exporter in exporters {
            let response = exporter
                .send("metrics-node-esdiag".to_string(), vec![serde_json::json!({"id": 1})])
                .await
                .expect("batch send");

            assert_eq!(response.docs, 1);
            assert_eq!(response.errors, 0);
            // `0` denotes "no HTTP transport", so it is what a successful
            // local write reports too — not just a failed one (ADR-0016).
            assert_eq!(response.status_code, 0);
        }
    }

    #[tokio::test]
    async fn document_channel_records_actual_failed_batch_count() {
        let tmp = TempDir::new().expect("temp dir");
        let exporter = Exporter::Archive(ArchiveExporter::zip(tmp.path().to_path_buf()).expect("archive exporter"));
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        tx.send(serde_json::json!({"id": 1})).await.expect("send doc");
        tx.send(serde_json::json!({"id": 2})).await.expect("send doc");
        tx.send(serde_json::json!({"id": 3})).await.expect("send doc");
        drop(tx);

        let summary = exporter
            .document_channel(rx, "metrics-node-esdiag".to_string(), 0)
            .await;

        let value = serde_json::to_value(summary).expect("summary json");
        assert_eq!(value["docs"], 0);
        assert_eq!(value["doc_errors"], 3);
        assert_eq!(value["batch"]["count"], 3);
    }

    #[tokio::test]
    async fn document_channel_streams_with_bounded_backpressure() {
        let tmp = TempDir::new().expect("temp dir");
        let exporter = Exporter::try_from(Uri::File(tmp.path().join("documents.ndjson"))).expect("file exporter");
        let (tx, rx) = tokio::sync::mpsc::channel(1);

        tx.try_send(serde_json::json!({"id": 1}))
            .expect("first document fills the bounded channel");
        assert!(
            matches!(
                tx.try_send(serde_json::json!({"id": 2})),
                Err(tokio::sync::mpsc::error::TrySendError::Full(_))
            ),
            "the producer must observe backpressure before the consumer starts"
        );

        let consumer = tokio::spawn(exporter.document_channel(rx, "metrics-node-esdiag".to_string(), 1));
        tokio::time::timeout(Duration::from_secs(1), tx.send(serde_json::json!({"id": 2})))
            .await
            .expect("the running exporter must drain while production continues")
            .expect("channel remains open");
        drop(tx);

        let summary = consumer.await.expect("document channel task");
        let value = serde_json::to_value(summary).expect("summary json");
        assert_eq!(value["docs"], 2);
        assert_eq!(value["doc_errors"], 0);
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
        assert_eq!(stream.outcome_uri(), "stdio://stdout");
    }

    #[test]
    fn kibana_link_prefers_saved_viewer_host() {
        let mut env = crate::TestEnv::new();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "send-host".to_string(),
            KnownHostBuilder::new(Url::parse("https://es.example:9200").expect("es url"))
                .application(Application::Elasticsearch)
                .roles(vec![HostRole::Send])
                .viewer(Some("viewer-host".to_string()))
                .build()
                .expect("send host"),
        );
        hosts.insert(
            "viewer-host".to_string(),
            KnownHostBuilder::new(Url::parse("https://kb.example:5601").expect("kb url"))
                .application(Application::Kibana)
                .roles(vec![HostRole::View])
                .build()
                .expect("viewer host"),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");
        env.set("ESDIAG_KIBANA_URL", "https://env-kb.example:5601");
        env.remove("ELASTIC_KIBANA_URL");
        env.remove("ESDIAG_KIBANA_SPACE");

        let exporter = Exporter::try_from(Uri::try_from("send-host").expect("host uri")).expect("exporter");
        let kibana_link = exporter
            .kibana_link(super::KibanaLinkTarget::ClusterReport, "diag-123", 1_700_000_000_000)
            .expect("kibana link");

        assert!(kibana_link.starts_with("https://kb.example:5601/s/esdiag/app/dashboards#/view/"));
        assert_eq!(exporter.outcome_uri(), "https://kb.example:5601/s/esdiag");
    }

    #[test]
    fn kibana_link_is_omitted_for_non_cluster_outputs() {
        let mut env = crate::TestEnv::new();
        env.set("ESDIAG_KIBANA_URL", "https://env-kb.example:5601");
        env.remove("ELASTIC_KIBANA_URL");
        env.set("ESDIAG_KIBANA_SPACE", "ops");

        let stream = Exporter::default();
        let directory = Exporter::try_from(Uri::Directory(env.tmp.path().to_path_buf())).expect("directory exporter");
        assert!(
            stream
                .kibana_link(super::KibanaLinkTarget::ClusterReport, "diag-123", 1_700_000_000_000)
                .is_none()
        );
        assert!(
            directory
                .kibana_link(super::KibanaLinkTarget::ClusterReport, "diag-123", 1_700_000_000_000)
                .is_none()
        );
    }

    #[test]
    fn kibana_link_ignores_elastic_kibana_env() {
        let mut env = crate::TestEnv::new();
        env.remove("ESDIAG_OUTPUT_URL");
        env.set("ELASTIC_ES_URL", "https://es.example:9200");
        env.remove("ESDIAG_KIBANA_URL");
        env.set("ELASTIC_KIBANA_URL", "https://elastic-kb.example:5601");

        let exporter = Exporter::try_from(Uri::KnownHost(
            KnownHostBuilder::new(Url::parse("https://es.example:9200").expect("es url"))
                .application(Application::Elasticsearch)
                .roles(vec![HostRole::Send])
                .build()
                .expect("send host"),
        ))
        .expect("cluster exporter");

        assert!(exporter.kibana_link("diag-123", 1_700_000_000_000).is_none());
    }

    #[test]
    fn environment_output_links_its_kibana_viewer() {
        let mut env = crate::TestEnv::new();
        env.set("ESDIAG_OUTPUT_URL", "https://es.example:9200");
        env.set("ESDIAG_KIBANA_URL", "https://env-kb.example:5601");
        env.set("ESDIAG_KIBANA_SPACE", "");

        let exporter = Exporter::try_from(Uri::try_from_output_env().expect("output env")).expect("cluster exporter");
        let kibana_link = exporter
            .kibana_link("diag-123", 1_700_000_000_000)
            .expect("kibana link");

        assert!(kibana_link.starts_with("https://env-kb.example:5601/app/dashboards#/view/"));
    }

    #[test]
    fn environment_viewer_is_not_attached_to_other_outputs() {
        let mut env = crate::TestEnv::new();
        env.set("ESDIAG_OUTPUT_URL", "https://es.example:9200");
        env.set("ESDIAG_KIBANA_URL", "https://env-kb.example:5601");

        let exporter = Exporter::try_from(Uri::KnownHost(
            KnownHostBuilder::new(Url::parse("https://es.example:9200").expect("es url"))
                .application(Application::Elasticsearch)
                .roles(vec![HostRole::Send])
                .build()
                .expect("send host"),
        ))
        .expect("cluster exporter");

        assert!(exporter.kibana_link("diag-123", 1_700_000_000_000).is_none());
    }

    #[test]
    fn kibana_links_omit_viewer_url_credentials() {
        let mut env = crate::TestEnv::new();
        env.set("ESDIAG_OUTPUT_URL", "https://es.example:9200");
        env.set("ESDIAG_KIBANA_URL", "https://viewer:hunter2@kb.example:5601");
        env.set("ESDIAG_KIBANA_SPACE", "");

        let exporter = Exporter::try_from(Uri::try_from_output_env().expect("output env")).expect("cluster exporter");
        let kibana_link = exporter
            .kibana_link("diag-123", 1_700_000_000_000)
            .expect("kibana link");

        assert!(
            kibana_link.starts_with("https://kb.example:5601/app/dashboards#/view/"),
            "{kibana_link}"
        );
        assert_eq!(exporter.outcome_uri(), "https://kb.example:5601");
        let Exporter::Elasticsearch(exporter) = exporter else {
            panic!("expected an Elasticsearch exporter");
        };
        let Some(crate::data::Uri::KnownHost(viewer)) = exporter.kibana_viewer.as_deref() else {
            panic!("expected the environment viewer");
        };
        assert_eq!(viewer.concrete_url().map(Url::password), Some(Some("hunter2")));
    }
}
