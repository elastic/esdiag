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
/// Get file from https://upload.elastic.co/
mod upload_service;

pub use elastic_cloud_admin::{ElasticCloudAdminReceiver, ElasticCloudAdminRequestError};
pub use elasticsearch::{ElasticsearchReceiver, ElasticsearchRequestError};
pub use kibana::{KibanaReceiver, KibanaRequestError};
pub use logstash::{LogstashReceiver, LogstashRequestError};

use super::{
    data::{KnownHost, Product, Uri},
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrubMode {
    Auto,
    Enabled,
    Disabled,
}

impl From<Option<bool>> for ScrubMode {
    fn from(value: Option<bool>) -> Self {
        match value {
            Some(true) => ScrubMode::Enabled,
            Some(false) => ScrubMode::Disabled,
            None => ScrubMode::Auto,
        }
    }
}

fn should_enable_scrubbed(mode: ScrubMode, filename: Option<&str>) -> bool {
    match mode {
        ScrubMode::Enabled => true,
        ScrubMode::Disabled => false,
        ScrubMode::Auto => filename
            .map(|value| {
                value
                    .split(|ch: char| !ch.is_ascii_alphanumeric())
                    .any(|token| token.eq_ignore_ascii_case("scrubbed"))
            })
            .unwrap_or(false),
    }
}

fn scrub_mode_label(mode: ScrubMode) -> &'static str {
    match mode {
        ScrubMode::Auto => "auto",
        ScrubMode::Enabled => "explicit true",
        ScrubMode::Disabled => "explicit false",
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
        Err(eyre!(
            "Manifest synthesis is not supported for this receiver"
        ))
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
        self.set_source_product_from_manifest(&manifest.product)?;
        Ok(manifest)
    }

    pub async fn try_get_manifest_from_files(&self) -> Result<DiagnosticManifest> {
        match self
            .read_bundle_json::<DiagnosticManifest>(DiagnosticManifest::FILENAME)
            .await
        {
            Ok(manifest) => {
                tracing::debug!("Using diagnostic_manifest.json");
                self.set_source_product_from_manifest(&manifest.product)?;
                return Ok(manifest);
            }
            Err(e) => tracing::debug!("Error reading diagnostic_manifest.json: {e}"),
        }

        match self.read_bundle_json::<Manifest>(Manifest::FILENAME).await {
            Ok(manifest) => {
                tracing::warn!("Falling back to manifest.json");
                let manifest: DiagnosticManifest = manifest.into();
                self.set_source_product_from_manifest(&manifest.product)?;
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

    fn set_source_product_from_manifest(&self, product: &Product) -> Result<()> {
        let Ok(product) = crate::processor::diagnostic::data_source::source_product_key(product) else {
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
        Receiver::try_from_with_scrub(uri, None, None)
    }
}

fn resolve_scrub_detect_name(path: &str, hint: Option<&str>) -> String {
    hint.filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.to_string())
}

impl Receiver {
    pub fn try_from_with_scrub(
        uri: Uri,
        scrubbed_override: Option<bool>,
        auto_detect_filename: Option<&str>,
    ) -> std::result::Result<Self, eyre::Report> {
        let scrub_mode = ScrubMode::from(scrubbed_override);
        let receiver = match uri {
            Uri::Directory(dir) => {
                let path_label = dir.to_string_lossy().to_string();
                let detect_name = resolve_scrub_detect_name(&path_label, auto_detect_filename);
                let mut receiver = DirectoryReceiver::try_from(dir)?;
                let scrubbed = should_enable_scrubbed(scrub_mode, Some(&detect_name));
                tracing::debug!(
                    "Scrub normalization {} for {} (detect name: {}, mode: {})",
                    if scrubbed { "enabled" } else { "disabled" },
                    path_label,
                    detect_name,
                    scrub_mode_label(scrub_mode)
                );
                receiver.set_scrubbed(scrubbed);
                Receiver::Directory(receiver)
            }
            Uri::ElasticCloud(host) => {
                return Err(eyre!("Elastic Cloud API not yet implemented. {host}"));
            }
            Uri::ElasticCloudAdmin(host) | Uri::ElasticGovCloudAdmin(host) => {
                Receiver::ElasticCloudAdmin(ElasticCloudAdminReceiver::try_from(host)?)
            }
            Uri::File(file) => {
                let path = file.to_string_lossy().to_string();
                let detect_name = resolve_scrub_detect_name(&path, auto_detect_filename);
                let mut receiver = ArchiveFileReceiver::try_from(file)?;
                let scrubbed = should_enable_scrubbed(scrub_mode, Some(&detect_name));
                tracing::debug!(
                    "Scrub normalization {} for {} (detect name: {}, mode: {})",
                    if scrubbed { "enabled" } else { "disabled" },
                    path,
                    detect_name,
                    scrub_mode_label(scrub_mode)
                );
                receiver.set_scrubbed(scrubbed);
                Receiver::ArchiveFile(receiver)
            }
            Uri::KnownHost(host) => match host.app() {
                Product::Elasticsearch => Receiver::Elasticsearch(ElasticsearchReceiver::try_from(host)?),
                Product::Logstash => Receiver::Logstash(LogstashReceiver::try_from(host)?),
                Product::Kibana => Receiver::Kibana(KibanaReceiver::try_from(host)?),
                _ => {
                    return Err(eyre!("Unsupported known-host receiver product: {}", host.app()));
                }
            },
            Uri::ServiceLink(url) => {
                let detect_name = auto_detect_filename
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                let mut receiver = UploadServiceDownloader::try_from(url)?.download()?;
                let scrubbed = should_enable_scrubbed(scrub_mode, detect_name.as_deref());
                if let Some(name) = detect_name.as_deref() {
                    tracing::debug!(
                        "Scrub normalization {} for service link (detect name: {}, mode: {})",
                        if scrubbed { "enabled" } else { "disabled" },
                        name,
                        scrub_mode_label(scrub_mode)
                    );
                }
                receiver.set_scrubbed(scrubbed);
                Receiver::ArchiveBytes(receiver)
            }
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
    use super::{DirectoryReceiver, Receiver, ScrubMode, resolve_scrub_detect_name, should_enable_scrubbed};

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
    fn upload_temp_path_auto_detects_from_original_filename_hint() {
        let temp = "esdiag-upload-1-550e8400-e29b-41d4-a716-446655440000.zip";
        let original = "example_scrubbed-api-diagnostics.zip";
        let detect = resolve_scrub_detect_name(temp, Some(original));
        assert!(should_enable_scrubbed(ScrubMode::Auto, Some(&detect)));
        assert!(!should_enable_scrubbed(ScrubMode::Auto, Some(temp)));
    }

    #[test]
    fn scrub_mode_enabled_always_true() {
        assert!(should_enable_scrubbed(ScrubMode::Enabled, None));
        assert!(should_enable_scrubbed(
            ScrubMode::Enabled,
            Some("diagnostic.zip")
        ));
    }

    #[test]
    fn scrub_mode_disabled_always_false() {
        assert!(!should_enable_scrubbed(ScrubMode::Disabled, None));
        assert!(!should_enable_scrubbed(
            ScrubMode::Disabled,
            Some("example_scrubbed-api-diagnostics.zip")
        ));
    }

    #[test]
    fn scrub_mode_auto_uses_filename_match() {
        assert!(should_enable_scrubbed(
            ScrubMode::Auto,
            Some("example_scrubbed-api-diagnostics.zip")
        ));
        assert!(should_enable_scrubbed(
            ScrubMode::Auto,
            Some("example-scrubbed-api-diagnostics.zip")
        ));
        assert!(should_enable_scrubbed(
            ScrubMode::Auto,
            Some("example.scrubbed.api.diagnostics.zip")
        ));
        assert!(!should_enable_scrubbed(
            ScrubMode::Auto,
            Some("example-unscrubbed-api-diagnostics.zip")
        ));
        assert!(!should_enable_scrubbed(ScrubMode::Auto, Some("example-api-diagnostics.zip")));
        assert!(!should_enable_scrubbed(ScrubMode::Auto, None));
    }
}
