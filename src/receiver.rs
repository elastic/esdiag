/// Read from a `.zip` archive file
mod archive;
/// Read from a direcotry in the local file system
mod directory;
/// Request API calls from Elasticsearch
mod elasticsearch;

use archive::ArchiveReceiver;
use directory::DirectoryReceiver;
pub use elasticsearch::ElasticsearchReceiver;

use crate::data::{
    diagnostic::{DataSource, DiagnosticManifest, Manifest},
    elasticsearch::Cluster,
    Uri,
};
use color_eyre::eyre::{eyre, Result};
use serde::de::DeserializeOwned;

trait Receive {
    #[allow(dead_code)]
    async fn is_connected(&self) -> bool;
    fn set_work_dir(&mut self, work_dir: &str) -> Result<()>;
    async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned;
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
/// - `Elasticsearch`: Requests data via API calls from an Elasticsearch service.
#[derive(Clone)]
pub enum Receiver {
    /// Read from a `.zip` archive file
    Archive(ArchiveReceiver),
    /// Read from a direcotry in the local file system
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
            Receiver::Archive(archive_receiver) => archive_receiver.get::<T>().await,
            Receiver::Directory(directory_receiver) => directory_receiver.get::<T>().await,
            Receiver::Elasticsearch(elasticsearch_receiver) => {
                elasticsearch_receiver.get::<T>().await
            }
        }
    }

    pub async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        match self {
            Receiver::Archive(archive_receiver) => archive_receiver.get_raw::<T>().await,
            Receiver::Directory(directory_receiver) => directory_receiver.get_raw::<T>().await,
            Receiver::Elasticsearch(elasticsearch_receiver) => {
                elasticsearch_receiver.get_raw::<T>().await
            }
        }
    }

    pub async fn is_connected(&self) -> bool {
        match self {
            Receiver::Archive(archive_receiver) => archive_receiver.is_connected().await,
            Receiver::Directory(directory_receiver) => directory_receiver.is_connected().await,
            Receiver::Elasticsearch(elasticsearch_receiver) => {
                elasticsearch_receiver.is_connected().await
            }
        }
    }

    pub fn set_work_dir(&mut self, work_dir: &str) -> Result<()> {
        match self {
            Receiver::Archive(archive_receiver) => archive_receiver.set_work_dir(work_dir),
            Receiver::Directory(directory_receiver) => directory_receiver.set_work_dir(work_dir),
            Receiver::Elasticsearch(elasticsearch_receiver) => {
                elasticsearch_receiver.set_work_dir(work_dir)
            }
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
            Ok(Manifest::try_from(version)?.try_into()?)
        }
    }
}

impl TryFrom<Uri> for Receiver {
    type Error = color_eyre::Report;
    fn try_from(uri: Uri) -> std::result::Result<Self, Self::Error> {
        match uri {
            Uri::Directory(dir) => Ok(Receiver::Directory(DirectoryReceiver::try_from(dir)?)),
            Uri::File(file) => Ok(Receiver::Archive(ArchiveReceiver::try_from(file)?)),
            Uri::Host(host) => Ok(Receiver::Elasticsearch(ElasticsearchReceiver::try_from(
                host,
            )?)),
            _ => Err(eyre!("Unsupported URI")),
        }
    }
}

impl std::fmt::Display for Receiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Receiver::Archive(archive_receiver) => write!(f, "file {}", archive_receiver),
            Receiver::Directory(directory_receiver) => write!(f, "file {}", directory_receiver),
            Receiver::Elasticsearch(elasticsearch_receiver) => {
                write!(f, "elasticsearch {}", elasticsearch_receiver)
            }
        }
    }
}
