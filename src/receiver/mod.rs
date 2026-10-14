// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Read from zip archives
mod archive;
/// Read from a direcotry in the local file system
mod directory;
/// Request API calls from the Elastic Cloud API proxy
mod elastic_cloud_admin;
/// Request API calls from Elasticsearch
mod elasticsearch;
/// Request API calls from Kibana
mod kibana;
/// Request API calls from Logstash
mod logstash;
/// Resolve stable and runtime-bound Phase-1 inputs.
mod resolver;
/// Get file from https://upload.elastic.co/
mod upload_service;

pub use elastic_cloud_admin::{ElasticCloudAdminReceiver, ElasticCloudAdminRequestError};
pub use elasticsearch::{ElasticsearchReceiver, ElasticsearchRequestError};
pub use kibana::{KibanaReceiver, KibanaRequestError};
pub use logstash::{LogstashReceiver, LogstashRequestError};
pub use resolver::{InputResolver, ResolvedInput};

use super::{
    data::{Application, KnownHost, Uri},
    processor::{DataSource, DiagnosticManifest, Manifest, SourceContext, StreamingDataSource},
};
use archive::{ArchiveBytesReceiver, ArchiveFileReceiver};
use directory::DirectoryReceiver;
use eyre::{Result, eyre};
use futures::stream::BoxStream;
use serde::de::DeserializeOwned;
use std::path::{Component, Path};
use std::time::Duration;
use upload_service::UploadServiceDownloader;

pub(crate) const LONG_RUNNING_REQUEST_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Debug)]
pub struct RawResponse {
    pub body: String,
    pub status: Option<u16>,
    pub response_time_ms: u64,
    pub response_size_bytes: u64,
}

/// A source a receiver can treat as absent during processing.
///
/// Processors match on this type to tell a legitimately absent source apart
/// from a source that is present but unreadable, so receivers must report
/// missing sources with this error rather than an ad-hoc message.
#[derive(Clone, Debug)]
pub enum MissingSource {
    /// None of the candidate filenames for the data source were present.
    NoCandidates { source: String },
    /// The archive did not contain the resolved entry path.
    ArchiveEntry { path: String },
    /// A resolved source file exists but contains no JSON value.
    Empty { path: String },
}

impl std::fmt::Display for MissingSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCandidates { source } => write!(f, "No candidate source files available for {source}"),
            Self::ArchiveEntry { path } => write!(f, "File not found in archive: {path}"),
            Self::Empty { path } => write!(f, "Source file is empty: {path}"),
        }
    }
}

impl std::error::Error for MissingSource {}

/// Send each line of `reader` without its line terminator. Invalid UTF-8 is
/// replaced rather than failing the whole file, because logs are not
/// guaranteed to be clean text.
fn send_bundle_lines(mut reader: impl std::io::BufRead, tx: &tokio::sync::mpsc::Sender<Result<String>>) {
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) => return,
            Ok(_) => {
                while matches!(buffer.last(), Some(b'\n' | b'\r')) {
                    buffer.pop();
                }
                let line = String::from_utf8_lossy(&buffer).into_owned();
                if tx.blocking_send(Ok(line)).is_err() {
                    return;
                }
            }
            Err(err) => {
                let _ = tx.blocking_send(Err(err.into()));
                return;
            }
        }
    }
}

fn bundle_line_stream(
    rx: tokio::sync::mpsc::Receiver<Result<String>>,
    handle: tokio::task::JoinHandle<()>,
    tx: tokio::sync::mpsc::Sender<Result<String>>,
) -> BoxStream<'static, Result<String>> {
    tokio::spawn(async move {
        if let Err(err) = handle.await
            && err.is_panic()
        {
            let _ = tx.send(Err(eyre!("Bundle line reader panicked"))).await;
        }
    });
    Box::pin(futures::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    }))
}

// Skip JSON whitespace without allocating a second copy of the source payload.
fn source_has_json(reader: &mut impl std::io::BufRead) -> std::io::Result<bool> {
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(false);
        }
        let whitespace = buffer
            .iter()
            .take_while(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
            .count();
        let found = whitespace < buffer.len();
        reader.consume(whitespace);
        if found {
            return Ok(true);
        }
    }
}

#[allow(async_fn_in_trait)]
pub trait Receive {
    async fn is_connected(&self) -> bool;
    async fn collection_date(&self) -> String;
    fn filename(&self) -> Option<String>;
    async fn get<T: DataSource + DeserializeOwned>(&self) -> Result<T>;
    async fn get_stream<T>(&self) -> Result<BoxStream<'static, Result<T::Item>>>
    where
        T: StreamingDataSource + DeserializeOwned,
        T::Item: DeserializeOwned + Send + 'static,
    {
        Err(eyre!("Streaming is not supported for this receiver"))
    }
    async fn try_get_manifest(&self) -> Result<DiagnosticManifest> {
        Err(eyre!("Manifest synthesis is not supported for this receiver"))
    }
}

pub trait ReceiveMultiple {
    fn set_work_dir(&mut self, work_dir: &str) -> Result<()>;
}

#[allow(async_fn_in_trait)]
pub trait ReceiveRaw {
    async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource;

    async fn get_raw_response<T>(&self) -> Result<RawResponse>
    where
        T: DataSource,
    {
        let body = self.get_raw::<T>().await?;
        let response_size_bytes = body.len() as u64;
        Ok(RawResponse {
            body,
            status: None,
            response_time_ms: 0,
            response_size_bytes,
        })
    }
}

/// The different types of receivers for data input.
///
/// This enum encapsulates various implementations of the `Receive` trait,
/// allowing for flexible handling of different data sources. Each variant
/// corresponds to a specific method of data retrieval:
///
/// - `Archive`: Reads data from a `.zip` archive file.
/// - `Directory`: Reads data from a directory in the local file system.
/// - `ElasticUploader`: Downloads an archive file from the Elastic Uploader service.
/// - `Elasticsearch`: Requests data via API calls from an Elasticsearch service.
/// - `RestApi`: Provides a REST API server that accepts diagnostic uploads.
#[derive(Clone)]
pub enum Receiver {
    /// Read from a `.zip` archive file
    ArchiveFile(ArchiveFileReceiver),
    /// Read from a `.zip` archive file
    ArchiveBytes(ArchiveBytesReceiver),
    /// Read from a directory in the local file system
    Directory(DirectoryReceiver),
    /// Request API calls from Elasticsearch
    Elasticsearch(ElasticsearchReceiver),
    /// Request API calls from Logstash
    Logstash(LogstashReceiver),
    /// Request API calls from Kibana
    Kibana(KibanaReceiver),
    /// Request API calls from Elastic Cloud admin
    ElasticCloudAdmin(ElasticCloudAdminReceiver),
}

impl Receiver {
    pub async fn source_context(&self) -> Result<SourceContext> {
        match self {
            Receiver::ArchiveBytes(receiver) => receiver.source_context(),
            Receiver::ArchiveFile(receiver) => receiver.source_context(),
            Receiver::Directory(receiver) => receiver.source_context(),
            Receiver::Elasticsearch(receiver) => Ok(SourceContext::new(
                "elasticsearch",
                Some(receiver.get_version().await?.clone()),
            )),
            Receiver::Logstash(receiver) => Ok(SourceContext::new(
                "logstash",
                Some(receiver.get_version().await?.clone()),
            )),
            Receiver::Kibana(receiver) => Ok(SourceContext::new(
                "kibana",
                Some(receiver.get_version().await?.clone()),
            )),
            Receiver::ElasticCloudAdmin(receiver) => Ok(SourceContext::new(
                "elasticsearch",
                Some(receiver.get_version().await?.clone()),
            )),
        }
    }

    #[tracing::instrument(skip(self))]
    pub async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        match self {
            Receiver::ArchiveBytes(receiver) => receiver.get::<T>().await,
            Receiver::ArchiveFile(receiver) => receiver.get::<T>().await,
            Receiver::Directory(receiver) => receiver.get::<T>().await,
            Receiver::Elasticsearch(receiver) => receiver.get::<T>().await,
            Receiver::Kibana(receiver) => receiver.get::<T>().await,
            Receiver::Logstash(receiver) => receiver.get::<T>().await,
            Receiver::ElasticCloudAdmin(receiver) => receiver.get::<T>().await,
        }
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_stream<T>(&self) -> Result<BoxStream<'static, Result<T::Item>>>
    where
        T: StreamingDataSource + DeserializeOwned,
        T::Item: DeserializeOwned + Send + 'static,
    {
        match self {
            Receiver::ArchiveBytes(receiver) => receiver.get_stream::<T>().await,
            Receiver::ArchiveFile(receiver) => receiver.get_stream::<T>().await,
            Receiver::Directory(receiver) => receiver.get_stream::<T>().await,
            Receiver::Elasticsearch(receiver) => receiver.get_stream::<T>().await,
            Receiver::Kibana(receiver) => receiver.get_stream::<T>().await,
            Receiver::Logstash(receiver) => receiver.get_stream::<T>().await,
            Receiver::ElasticCloudAdmin(receiver) => receiver.get_stream::<T>().await,
        }
    }

    pub async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        self.get_raw_response::<T>().await.map(|response| response.body)
    }

    pub async fn get_raw_response<T>(&self) -> Result<RawResponse>
    where
        T: DataSource,
    {
        match self {
            Receiver::Elasticsearch(receiver) => receiver.get_raw_response::<T>().await,
            Receiver::Kibana(receiver) => receiver.get_raw_response::<T>().await,
            Receiver::Logstash(receiver) => receiver.get_raw_response::<T>().await,
            Receiver::ElasticCloudAdmin(receiver) => receiver.get_raw_response::<T>().await,
            _ => Err(eyre!("Raw data is not supported for this receiver")),
        }
    }

    pub async fn get_raw_by_path(&self, path: &str, extension: &str) -> Result<String> {
        self.get_raw_response_by_path(path, extension)
            .await
            .map(|response| response.body)
    }

    pub async fn get_raw_response_by_path(&self, path: &str, extension: &str) -> Result<RawResponse> {
        match self {
            Receiver::Elasticsearch(receiver) => receiver.get_raw_response_by_path(path, extension).await,
            Receiver::Kibana(receiver) => receiver.get_raw_response_by_path(path, extension).await,
            Receiver::Logstash(receiver) => receiver.get_raw_response_by_path(path, extension).await,
            Receiver::ElasticCloudAdmin(receiver) => receiver.get_raw_response_by_path(path, extension).await,
            _ => Err(eyre!(
                "Raw data by path is only supported for Elasticsearch, Elastic Cloud Admin, Kibana, or Logstash receivers"
            )),
        }
    }

    pub async fn is_connected(&self) -> bool {
        match self {
            Receiver::ArchiveBytes(receiver) => receiver.is_connected().await,
            Receiver::ArchiveFile(receiver) => receiver.is_connected().await,
            Receiver::Directory(receiver) => receiver.is_connected().await,
            Receiver::Elasticsearch(receiver) => receiver.is_connected().await,
            Receiver::Kibana(receiver) => receiver.is_connected().await,
            Receiver::Logstash(receiver) => receiver.is_connected().await,
            Receiver::ElasticCloudAdmin(receiver) => receiver.is_connected().await,
        }
    }

    pub fn set_work_dir(&mut self, work_dir: &str) -> Result<()> {
        match self {
            Receiver::ArchiveBytes(reciever) => reciever.set_work_dir(work_dir),
            Receiver::ArchiveFile(reciever) => reciever.set_work_dir(work_dir),
            Receiver::Directory(reciever) => reciever.set_work_dir(work_dir),
            _ => Err(eyre!("Cannot set working directly on {}", self)),
        }
    }

    pub fn clone_for_subdir(&self, sub_dir: &str) -> Result<Self> {
        validate_relative_subdir(sub_dir)?;
        match self {
            Receiver::ArchiveBytes(receiver) => Ok(Receiver::ArchiveBytes(receiver.clone_for_subdir(sub_dir))),
            Receiver::ArchiveFile(receiver) => Ok(Receiver::ArchiveFile(receiver.clone_for_subdir(sub_dir))),
            Receiver::Directory(receiver) => Ok(Receiver::Directory(receiver.clone_for_subdir(sub_dir))),
            _ => {
                let mut receiver = self.clone();
                receiver.set_work_dir(sub_dir)?;
                Ok(receiver)
            }
        }
    }

    pub async fn collection_date(&self) -> String {
        match self {
            Receiver::ArchiveBytes(receiver) => receiver.collection_date().await,
            Receiver::ArchiveFile(receiver) => receiver.collection_date().await,
            Receiver::Directory(receiver) => receiver.collection_date().await,
            Receiver::Elasticsearch(receiver) => receiver.collection_date().await,
            Receiver::Kibana(receiver) => receiver.collection_date().await,
            Receiver::Logstash(receiver) => receiver.collection_date().await,
            Receiver::ElasticCloudAdmin(receiver) => receiver.collection_date().await,
        }
    }

    pub fn filename(&self) -> Option<String> {
        match self {
            Receiver::ArchiveBytes(receiver) => receiver.filename(),
            Receiver::ArchiveFile(receiver) => receiver.filename(),
            Receiver::Directory(receiver) => receiver.filename(),
            Receiver::Elasticsearch(receiver) => receiver.filename(),
            Receiver::Kibana(receiver) => receiver.filename(),
            Receiver::Logstash(receiver) => receiver.filename(),
            Receiver::ElasticCloudAdmin(receiver) => receiver.filename(),
        }
    }

    /// A platform the receiver itself can vouch for, independent of manifest
    /// indicators (ADR-0001: platform is determined best-effort at the
    /// receiver). Collecting through the Elastic Cloud admin API implies
    /// Elastic Cloud Hosted.
    pub fn platform_hint(&self) -> Option<crate::data::Platform> {
        match self {
            Receiver::ElasticCloudAdmin(_) => Some(crate::data::Platform::ElasticCloudHosted),
            _ => None,
        }
    }

    /// Whether the receiver reads a collected bundle rather than a live API.
    /// Only these receivers can be asked about bundle contents.
    pub fn is_bundle(&self) -> bool {
        matches!(
            self,
            Receiver::ArchiveBytes(_) | Receiver::ArchiveFile(_) | Receiver::Directory(_)
        )
    }

    /// Whether the received bundle contains `dir` as a directory. Only local
    /// (archive/directory) receivers can answer; remote receivers report
    /// `false`. Used for platform indicators such as the `syscalls` folder.
    pub async fn has_bundle_dir(&self, dir: &str) -> bool {
        match self {
            Receiver::ArchiveBytes(receiver) => receiver.has_bundle_dir(dir).await,
            Receiver::ArchiveFile(receiver) => receiver.has_bundle_dir(dir).await,
            Receiver::Directory(receiver) => receiver.has_bundle_dir(dir),
            _ => false,
        }
    }

    pub async fn try_get_manifest(&self) -> Result<DiagnosticManifest> {
        let manifest = match self {
            Receiver::ArchiveBytes(_) | Receiver::ArchiveFile(_) | Receiver::Directory(_) => {
                self.try_get_manifest_from_files().await
            }
            Receiver::Elasticsearch(receiver) => receiver.try_get_manifest().await,
            Receiver::Kibana(receiver) => receiver.try_get_manifest().await,
            Receiver::Logstash(receiver) => receiver.try_get_manifest().await,
            Receiver::ElasticCloudAdmin(receiver) => receiver.try_get_manifest().await,
        }?;
        self.set_source_application_from_manifest(manifest.application())?;
        Ok(manifest)
    }

    pub async fn try_get_manifest_from_files(&self) -> Result<DiagnosticManifest> {
        match self
            .read_bundle_json::<DiagnosticManifest>(DiagnosticManifest::FILENAME)
            .await
        {
            Ok(manifest) => {
                tracing::debug!("Using diagnostic_manifest.json");
                self.set_source_application_from_manifest(manifest.application())?;
                return Ok(manifest);
            }
            Err(e) => tracing::debug!("Error reading diagnostic_manifest.json: {e}"),
        }

        match self.read_bundle_json::<Manifest>(Manifest::FILENAME).await {
            Ok(manifest) => {
                tracing::warn!("Falling back to manifest.json");
                let manifest: DiagnosticManifest = manifest.into();
                self.set_source_application_from_manifest(manifest.application())?;
                Ok(manifest)
            }
            Err(e) => Err(eyre!("Failed to identify product from diagnostic manifest: {}", e)),
        }
    }

    pub async fn read_bundle_json<T>(&self, filename: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        match self {
            Receiver::ArchiveBytes(receiver) => receiver.read_bundle_json(filename).await,
            Receiver::ArchiveFile(receiver) => receiver.read_bundle_json(filename).await,
            Receiver::Directory(receiver) => receiver.read_bundle_json(filename).await,
            _ => Err(eyre!(
                "Bundle file reads are only supported for archive and directory receivers"
            )),
        }
    }

    /// Names of the directories directly inside `dir`, relative to the
    /// receiver's working directory; `""` lists the bundle root.
    pub async fn list_bundle_dirs(&self, dir: &str) -> Result<Vec<String>> {
        match self {
            Receiver::ArchiveBytes(receiver) => receiver.list_bundle_dirs(dir).await,
            Receiver::ArchiveFile(receiver) => receiver.list_bundle_dirs(dir).await,
            Receiver::Directory(receiver) => receiver.list_bundle_dirs(dir),
            _ => Err(eyre!(
                "Bundle directory listing is only supported for archive and directory receivers"
            )),
        }
    }

    /// Stream a bundle text file line by line without loading it into memory.
    /// A missing file fails before the stream is returned.
    pub async fn read_bundle_lines(&self, path: &str) -> Result<BoxStream<'static, Result<String>>> {
        match self {
            Receiver::ArchiveBytes(receiver) => receiver.read_bundle_lines(path).await,
            Receiver::ArchiveFile(receiver) => receiver.read_bundle_lines(path).await,
            Receiver::Directory(receiver) => receiver.read_bundle_lines(path),
            _ => Err(eyre!(
                "Bundle file reads are only supported for archive and directory receivers"
            )),
        }
    }

    fn set_source_application_from_manifest(&self, application: Option<Application>) -> Result<()> {
        let Some(application) = application else {
            return Ok(());
        };
        let Ok(product) = crate::processor::diagnostic::data_source::source_application_key(application) else {
            return Ok(());
        };

        match self {
            Receiver::ArchiveBytes(receiver) => receiver.set_source_product(product),
            Receiver::ArchiveFile(receiver) => receiver.set_source_product(product),
            Receiver::Directory(receiver) => receiver.set_source_product(product),
            _ => Ok(()),
        }
    }
}

fn validate_relative_subdir(sub_dir: &str) -> Result<()> {
    let path = Path::new(sub_dir);
    if path.as_os_str().is_empty() {
        return Err(eyre!("Included diagnostic path cannot be empty"));
    }

    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(eyre!(
                    "Included diagnostic path must be relative and stay within the bundle"
                ));
            }
        }
    }

    Ok(())
}

impl TryFrom<Uri> for Receiver {
    type Error = eyre::Report;
    fn try_from(uri: Uri) -> std::result::Result<Self, Self::Error> {
        let receiver = match uri {
            Uri::Directory(dir) => Receiver::Directory(DirectoryReceiver::try_from(dir)?),
            Uri::ElasticCloud(host) => {
                return Err(eyre!("Elastic Cloud API not yet implemented. {host}"));
            }
            Uri::ElasticCloudAdmin(host) | Uri::ElasticGovCloudAdmin(host) => {
                Receiver::ElasticCloudAdmin(ElasticCloudAdminReceiver::try_from(host)?)
            }
            Uri::File(file) => Receiver::ArchiveFile(ArchiveFileReceiver::try_from(file)?),
            Uri::KnownHost(host) => {
                let resolved = host.resolve()?;
                let application = resolved.application();
                let host = resolved.into_known_host();
                match application {
                    Application::Elasticsearch => Receiver::Elasticsearch(ElasticsearchReceiver::try_from(host)?),
                    Application::Logstash => Receiver::Logstash(LogstashReceiver::try_from(host)?),
                    Application::Kibana => Receiver::Kibana(KibanaReceiver::try_from(host)?),
                    application => {
                        unreachable!("KnownHost::resolve returned non-collectable application {application}")
                    }
                }
            }
            Uri::ServiceLink(url) => Receiver::ArchiveBytes(UploadServiceDownloader::try_from(url)?.download()?),
            _ => return Err(eyre!("Unsupported URI: {uri}")),
        };
        Ok(receiver)
    }
}

impl TryFrom<KnownHost> for Receiver {
    type Error = eyre::Report;
    fn try_from(host: KnownHost) -> std::result::Result<Self, Self::Error> {
        let uri = Uri::try_from(host)?;
        Receiver::try_from(uri)
    }
}

impl TryFrom<bytes::Bytes> for Receiver {
    type Error = eyre::Report;
    fn try_from(bytes: bytes::Bytes) -> std::result::Result<Self, Self::Error> {
        Ok(Receiver::ArchiveBytes(ArchiveBytesReceiver::try_from(bytes)?))
    }
}

impl std::fmt::Display for Receiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Receiver::ArchiveBytes(receiver) => write!(f, "Archive Bytes {receiver}"),
            Receiver::ArchiveFile(receiver) => write!(f, "Archive File {receiver}"),
            Receiver::Directory(receiver) => write!(f, "Directory {receiver}"),
            Receiver::Elasticsearch(receiver) => write!(f, "Elasticsearch {receiver}"),
            Receiver::Kibana(receiver) => write!(f, "Kibana {receiver}"),
            Receiver::Logstash(receiver) => write!(f, "Logstash {receiver}"),
            Receiver::ElasticCloudAdmin(receiver) => {
                write!(f, "ElasticCloudAdmin {receiver}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DirectoryReceiver, Receiver};
    use crate::data::{Application, KnownHostBuilder};
    use url::Url;

    #[test]
    fn source_probe_handles_whitespace_across_buffers_without_consuming_json() {
        for input in ["", " \n\r\t \n"] {
            let mut reader = std::io::BufReader::with_capacity(2, input.as_bytes());
            assert!(!super::source_has_json(&mut reader).unwrap());
        }
        let mut reader = std::io::BufReader::with_capacity(2, b" \n\r\t {\"value\":42}  ".as_slice());
        assert!(super::source_has_json(&mut reader).unwrap());
        let value: serde_json::Value = serde_json::from_reader(reader).unwrap();
        assert_eq!(value, serde_json::json!({"value":42}));
    }

    #[test]
    fn source_probe_leaves_malformed_or_trailing_json_as_parse_errors() {
        for input in [" {", "null null", "invalid"] {
            let mut reader = std::io::BufReader::with_capacity(2, input.as_bytes());
            assert!(super::source_has_json(&mut reader).unwrap());
            assert!(serde_json::from_reader::<_, serde_json::Value>(reader).is_err());
        }
    }

    fn eck_fixture_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/archives/eck-diagnostics-synthetic")
    }

    /// Zip the ECK fixture beneath a single root folder, the layout a
    /// re-packaged bundle commonly has.
    fn zipped_eck_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        use std::io::Write;
        let tmp = tempfile::tempdir().expect("zip dir");
        let zip_path = tmp.path().join("eck.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).expect("create zip"));
        let root = eck_fixture_dir();
        let mut files = Vec::new();
        let mut pending = vec![root.clone()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir).expect("read fixture dir") {
                let path = entry.expect("fixture entry").path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    files.push(path);
                }
            }
        }
        files.sort_by_key(|path| (path.parent() != Some(root.as_path()), path.clone()));
        for path in files {
            let relative = path
                .strip_prefix(&root)
                .expect("relative")
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(format!("eck-root/{relative}"), zip::write::SimpleFileOptions::default())
                .expect("start file");
            zip.write_all(&std::fs::read(&path).expect("read fixture file"))
                .expect("write file");
        }
        zip.finish().expect("finish zip");
        (tmp, zip_path)
    }

    fn eck_fixture_receivers() -> (tempfile::TempDir, Vec<Receiver>) {
        let (tmp, zip_path) = zipped_eck_fixture();
        let receivers = vec![
            Receiver::Directory(DirectoryReceiver::try_from(eck_fixture_dir()).expect("directory receiver")),
            Receiver::ArchiveFile(super::ArchiveFileReceiver::try_from(zip_path.clone()).expect("archive receiver")),
            Receiver::try_from(bytes::Bytes::from(std::fs::read(&zip_path).expect("zip bytes")))
                .expect("bytes receiver"),
        ];
        (tmp, receivers)
    }

    #[tokio::test]
    async fn bundle_dirs_list_namespaces_and_pods_for_every_bundle_receiver() {
        let (_tmp, receivers) = eck_fixture_receivers();
        for receiver in receivers {
            assert_eq!(
                receiver.list_bundle_dirs("").await.expect("root dirs"),
                vec!["demo".to_string(), "elastic-system".to_string()],
                "{receiver}"
            );
            assert_eq!(
                receiver.list_bundle_dirs("elastic-system/pod").await.expect("pod dirs"),
                vec!["elastic-operator-0".to_string()],
                "{receiver}"
            );
            assert!(
                receiver.list_bundle_dirs("absent").await.is_err(),
                "{receiver} must report a missing directory"
            );
        }
    }

    #[tokio::test]
    async fn bundle_lines_stream_without_terminators_for_every_bundle_receiver() {
        use futures::StreamExt;
        let (_tmp, receivers) = eck_fixture_receivers();
        for receiver in receivers {
            let lines: Vec<String> = receiver
                .read_bundle_lines("elastic-system/pod/elastic-operator-0/logs.txt")
                .await
                .expect("line stream")
                .map(|line| line.expect("line"))
                .collect()
                .await;
            assert_eq!(lines.len(), 10, "{receiver}");
            assert!(lines[0].starts_with("==== START logs"), "{receiver}");
            assert!(lines.iter().all(|line| !line.ends_with('\n') && !line.ends_with('\r')));

            let missing = receiver.read_bundle_lines("elastic-system/pod/absent/logs.txt").await;
            assert!(
                missing.is_err_and(|err| err.chain().any(|cause| cause.is::<super::MissingSource>()
                    || cause
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|err| err.kind() == std::io::ErrorKind::NotFound))),
                "{receiver} must report a missing file as a missing source"
            );
        }
    }

    #[tokio::test]
    async fn empty_bundle_json_is_a_missing_source_for_every_bundle_receiver() {
        let (_tmp, receivers) = eck_fixture_receivers();
        for receiver in receivers {
            let err = receiver
                .read_bundle_json::<serde_json::Value>("podsecuritypolicies.json")
                .await
                .expect_err("empty file");
            assert!(err.is::<super::MissingSource>(), "{receiver}: {err}");
        }
    }

    fn directory_receiver() -> Receiver {
        let root = tempfile::tempdir().expect("temp diagnostic root");
        Receiver::Directory(DirectoryReceiver::try_from(root.keep()).expect("directory receiver"))
    }

    #[test]
    fn clone_for_subdir_accepts_relative_bundle_path() {
        let receiver = directory_receiver();

        assert!(receiver.clone_for_subdir("namespace/es/instance").is_ok());
    }

    #[test]
    fn clone_for_subdir_rejects_parent_traversal() {
        let receiver = directory_receiver();

        let err = receiver
            .clone_for_subdir("namespace/../outside")
            .err()
            .expect("parent traversal should be rejected");

        assert!(err.to_string().contains("must be relative and stay within the bundle"));
    }

    #[test]
    fn clone_for_subdir_rejects_absolute_path() {
        let receiver = directory_receiver();

        let err = receiver
            .clone_for_subdir("/tmp/outside")
            .err()
            .expect("absolute path should be rejected");

        assert!(err.to_string().contains("must be relative and stay within the bundle"));
    }

    #[test]
    fn known_host_receiver_refuses_agent_collect_by_design() {
        let host = KnownHostBuilder::new(Url::parse("http://localhost:8220").expect("url"))
            .application(Application::Agent)
            .build()
            .expect("host");

        let err = Receiver::try_from(host).err().expect("agent collect should be refused");

        assert!(err.to_string().contains("out of scope by design for Agent"));
        assert!(err.to_string().contains("read/Load"));
    }
}
