/// Read from zip archives
mod archive;
/// Read from a direcotry in the local file system
mod directory;
/// Request API calls from Elasticsearch
mod elasticsearch;
/// Get file from https://upload.elastic.co/
mod upload_service;

use archive::{ArchiveBytesReceiver, ArchiveFileReceiver};
use directory::DirectoryReceiver;
pub use elasticsearch::ElasticsearchReceiver;
use upload_service::UploadServiceDownloader;

use super::{
    data::Uri,
    processor::{DataSource, DiagnosticManifest, ElasticsearchCluster, Manifest, ManifestBuilder},
};
use eyre::{Result, eyre};
use serde::de::DeserializeOwned;

#[allow(async_fn_in_trait)]
pub trait Receive {
    async fn is_connected(&self) -> bool;
    async fn collection_date(&self) -> String;
    async fn get<T: DataSource + DeserializeOwned>(&self) -> Result<T>;
    async fn try_get_manifest(&self) -> Result<DiagnosticManifest> {
        match self.get::<DiagnosticManifest>().await {
            Ok(manifest) => {
                log::debug!("Using diagnostic_manifest.json");
                return Ok(manifest);
            }
            Err(e) => log::debug!("Error reading diagnostic_manifest.json: {e}"),
        }

        match self.get::<Manifest>().await {
            Ok(manifest) => {
                log::warn!("Falling back to manifest.json");
                return Ok(manifest.try_into()?);
            }
            Err(e) => log::debug!("Error reading manifest.json: {e}"),
        }

        let cluster = self.get::<ElasticsearchCluster>().await?;
        let collection_date = self.collection_date().await;
        Ok(ManifestBuilder::from(cluster)
            .collection_date(collection_date)
            .build()
            .try_into()?)
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
}

impl Receiver {
    pub async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        match self {
            Receiver::ArchiveBytes(receiver) => receiver.get::<T>().await,
            Receiver::ArchiveFile(receiver) => receiver.get::<T>().await,
            Receiver::Directory(receiver) => receiver.get::<T>().await,
            Receiver::Elasticsearch(receiver) => receiver.get::<T>().await,
        }
    }

    pub async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        match self {
            Receiver::Elasticsearch(receiver) => receiver.get_raw::<T>().await,
            _ => Err(eyre!("Raw data is not supported for this receiver")),
        }
    }

    pub async fn is_connected(&self) -> bool {
        match self {
            Receiver::ArchiveBytes(receiver) => receiver.is_connected().await,
            Receiver::ArchiveFile(receiver) => receiver.is_connected().await,
            Receiver::Directory(receiver) => receiver.is_connected().await,
            Receiver::Elasticsearch(receiver) => receiver.is_connected().await,
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
        let mut receiver = self.clone();
        receiver.set_work_dir(sub_dir)?;
        Ok(receiver)
    }

    pub async fn collection_date(&self) -> String {
        match self {
            Receiver::ArchiveBytes(receiver) => receiver.collection_date().await,
            Receiver::ArchiveFile(receiver) => receiver.collection_date().await,
            Receiver::Directory(receiver) => receiver.collection_date().await,
            Receiver::Elasticsearch(receiver) => receiver.collection_date().await,
        }
    }

    pub async fn try_get_manifest(&self) -> Result<DiagnosticManifest> {
        match self {
            Receiver::ArchiveBytes(receiver) => receiver.try_get_manifest().await,
            Receiver::ArchiveFile(receiver) => receiver.try_get_manifest().await,
            Receiver::Directory(receiver) => receiver.try_get_manifest().await,
            Receiver::Elasticsearch(receiver) => receiver.try_get_manifest().await,
        }
    }
}

impl TryFrom<Uri> for Receiver {
    type Error = eyre::Report;
    fn try_from(uri: Uri) -> std::result::Result<Self, Self::Error> {
        let receiver = match uri {
            Uri::Directory(dir) => Receiver::Directory(DirectoryReceiver::try_from(dir)?),
            Uri::File(file) => Receiver::ArchiveFile(ArchiveFileReceiver::try_from(file)?),
            Uri::KnownHost(host) => Receiver::Elasticsearch(ElasticsearchReceiver::try_from(host)?),
            Uri::ElasticUploader(url) => {
                Receiver::ArchiveBytes(UploadServiceDownloader::try_from(url)?.download()?)
            }
            _ => return Err(eyre!("Unsupported URI: {uri}")),
        };
        Ok(receiver)
    }
}

impl TryFrom<bytes::Bytes> for Receiver {
    type Error = eyre::Report;
    fn try_from(bytes: bytes::Bytes) -> std::result::Result<Self, Self::Error> {
        Ok(Receiver::ArchiveBytes(ArchiveBytesReceiver::try_from(
            bytes,
        )?))
    }
}

impl std::fmt::Display for Receiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Receiver::ArchiveBytes(receiver) => write!(f, "Archive Bytes {receiver}"),
            Receiver::ArchiveFile(receiver) => write!(f, "Archive File {receiver}"),
            Receiver::Directory(receiver) => write!(f, "Directory {receiver}"),
            Receiver::Elasticsearch(receiver) => write!(f, "Elasticsearch {receiver}"),
        }
    }
}
