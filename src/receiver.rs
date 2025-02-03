/// Read from a `.zip` archive file
mod archive;
/// Read from a direcotry in the local file system
mod directory;
/// Get file from Elastic Uploader service
mod elastic_uploader;
/// Request API calls from Elasticsearch
mod elasticsearch;

use archive::ArchiveReceiver;
use directory::DirectoryReceiver;
use elastic_uploader::ElasticUploaderReceiver;
pub use elasticsearch::ElasticsearchReceiver;

use crate::data::{
    diagnostic::{DataSource, DiagnosticManifest, Manifest},
    elasticsearch::Cluster,
    Uri,
};
use color_eyre::eyre::{eyre, Result};
use serde::de::DeserializeOwned;

trait Receive {
    async fn is_connected(&self) -> bool;
    async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned;
}

trait ReceiveMultiple {
    fn set_work_dir(&mut self, work_dir: &str) -> Result<()>;
}

trait ReceiveRaw {
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
#[derive(Clone)]
pub enum Receiver {
    /// Read from a `.zip` archive file
    Archive(ArchiveReceiver),
    /// Read from a direcotry in the local file system
    Directory(DirectoryReceiver),
    /// Get file from Elastic Uploader service
    ElasticUploader(ElasticUploaderReceiver),
    /// Request API calls from Elasticsearch
    Elasticsearch(ElasticsearchReceiver),
}

impl Receiver {
    pub async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        match self {
            Receiver::Archive(receiver) => receiver.get::<T>().await,
            Receiver::Directory(receiver) => receiver.get::<T>().await,
            Receiver::ElasticUploader(receiver) => receiver.get::<T>().await,
            Receiver::Elasticsearch(receiver) => receiver.get::<T>().await,
        }
    }

    pub async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        match self {
            Receiver::Archive(receiver) => receiver.get_raw::<T>().await,
            Receiver::Elasticsearch(receiver) => receiver.get_raw::<T>().await,
            _ => Err(eyre!("Raw data is not supported for this receiver")),
        }
    }

    pub async fn is_connected(&self) -> bool {
        match self {
            Receiver::Archive(receiver) => receiver.is_connected().await,
            Receiver::Directory(receiver) => receiver.is_connected().await,
            Receiver::Elasticsearch(receiver) => receiver.is_connected().await,
            Receiver::ElasticUploader(receiver) => receiver.is_connected().await,
        }
    }

    pub fn set_work_dir(&mut self, work_dir: &str) -> Result<()> {
        match self {
            Receiver::Archive(reciever) => reciever.set_work_dir(work_dir),
            Receiver::Directory(reciever) => reciever.set_work_dir(work_dir),
            _ => Err(eyre!("Cannot set working directly on {}", self)),
        }
    }

    pub fn clone_for_subdir(&self, sub_dir: &str) -> Result<Self> {
        let mut receiver = self.clone();
        receiver.set_work_dir(sub_dir)?;
        Ok(receiver)
    }

    pub async fn try_get_manifest(&self) -> Result<DiagnosticManifest> {
        if let Ok(manifest) = self.get::<DiagnosticManifest>().await {
            log::debug!("Using diagnostic_manifest.json");
            Ok(manifest)
        } else if let Ok(manifest) = self.get::<Manifest>().await {
            log::warn!("Falling back to manifest.json");
            Ok(manifest.try_into()?)
        } else {
            log::warn!("Falling back to version.json");
            let version = self.get::<Cluster>().await?;
            let manifest = match self {
                Receiver::Elasticsearch(_) => Manifest::try_from(version)?.with_runner("esdiag"),
                _ => Manifest::try_from(version)?,
            };
            Ok(manifest.try_into()?)
        }
    }
}

impl TryFrom<Uri> for Receiver {
    type Error = color_eyre::Report;
    fn try_from(uri: Uri) -> std::result::Result<Self, Self::Error> {
        let receiver = match uri {
            Uri::Directory(dir) => Receiver::Directory(DirectoryReceiver::try_from(dir)?),
            Uri::File(file) => Receiver::Archive(ArchiveReceiver::try_from(file)?),
            Uri::KnownHost(host) => Receiver::Elasticsearch(ElasticsearchReceiver::try_from(host)?),
            Uri::ElasticUploader(url) => {
                Receiver::ElasticUploader(ElasticUploaderReceiver::try_from(url)?)
            }
            _ => return Err(eyre!("Unsupported URI: {uri}")),
        };
        Ok(receiver)
    }
}

impl std::fmt::Display for Receiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Receiver::Archive(receiver) => write!(f, "File {receiver}"),
            Receiver::Directory(receiver) => write!(f, "Directory {receiver}"),
            Receiver::ElasticUploader(receiver) => write!(f, "Elastic Uploader {receiver}"),
            Receiver::Elasticsearch(receiver) => write!(f, "Elasticsearch {receiver}"),
        }
    }
}
