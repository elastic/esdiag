/// API server that accepts diagnostic uploads via HTTP form
mod api_server;
/// Read from a `.zip` archive file
mod archive;
/// Read from a direcotry in the local file system
mod directory;
/// Get file from Elastic Uploader service
mod elastic_uploader;
/// Request API calls from Elasticsearch
mod elasticsearch;

pub use api_server::ApiReceiver;
use archive::ArchiveReceiver;
use directory::DirectoryReceiver;
use elastic_uploader::ElasticUploaderReceiver;
pub use elasticsearch::ElasticsearchReceiver;

use crate::data::{
    Uri,
    diagnostic::{DataSource, DiagnosticManifest, Manifest, manifest::ManifestBuilder},
    elasticsearch::Cluster,
};
use eyre::{Result, eyre};
use serde::de::DeserializeOwned;

#[allow(async_fn_in_trait)]
pub trait Receive {
    async fn is_connected(&self) -> bool;
    async fn collection_date(&self) -> String;
    async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned;
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
    Archive(ArchiveReceiver),
    /// Read from a direcotry in the local file system
    Directory(DirectoryReceiver),
    /// Get file from Elastic Uploader service
    ElasticUploader(ElasticUploaderReceiver),
    /// Request API calls from Elasticsearch
    Elasticsearch(ElasticsearchReceiver),
    /// REST API server that accepts diagnostic uploads
    ApiServer(ApiReceiver),
}

impl Receiver {
    pub fn new_api_server(port: u16) -> Self {
        Receiver::ApiServer(ApiReceiver::new(port))
    }

    pub async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        match self {
            Receiver::Archive(receiver) => receiver.get::<T>().await,
            Receiver::Directory(receiver) => receiver.get::<T>().await,
            Receiver::ElasticUploader(receiver) => receiver.get::<T>().await,
            Receiver::Elasticsearch(receiver) => receiver.get::<T>().await,
            Receiver::ApiServer(receiver) => receiver.get::<T>().await,
        }
    }

    pub async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        match self {
            Receiver::Archive(receiver) => receiver.get_raw::<T>().await,
            Receiver::Elasticsearch(receiver) => receiver.get_raw::<T>().await,
            Receiver::ApiServer(receiver) => receiver.get_raw::<T>().await,
            _ => Err(eyre!("Raw data is not supported for this receiver")),
        }
    }

    pub async fn is_connected(&self) -> bool {
        match self {
            Receiver::Archive(receiver) => receiver.is_connected().await,
            Receiver::Directory(receiver) => receiver.is_connected().await,
            Receiver::Elasticsearch(receiver) => receiver.is_connected().await,
            Receiver::ElasticUploader(receiver) => receiver.is_connected().await,
            Receiver::ApiServer(receiver) => receiver.is_connected().await,
        }
    }

    pub fn set_work_dir(&mut self, work_dir: &str) -> Result<()> {
        match self {
            Receiver::Archive(reciever) => reciever.set_work_dir(work_dir),
            Receiver::Directory(reciever) => reciever.set_work_dir(work_dir),
            Receiver::ElasticUploader(reciever) => reciever.set_work_dir(work_dir),
            Receiver::ApiServer(reciever) => reciever.set_work_dir(work_dir),
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
            Receiver::Archive(receiver) => receiver.collection_date().await,
            Receiver::Directory(receiver) => receiver.collection_date().await,
            Receiver::Elasticsearch(receiver) => receiver.collection_date().await,
            Receiver::ElasticUploader(receiver) => receiver.collection_date().await,
            Receiver::ApiServer(receiver) => receiver.collection_date().await,
        }
    }

    pub async fn try_get_manifest(&self) -> Result<DiagnosticManifest> {
        match self.get::<DiagnosticManifest>().await {
            Ok(manifest) => {
                log::debug!("Using diagnostic_manifest.json");
                return Ok(manifest);
            }
            Err(e) => {
                log::debug!("Error reading diagnostic_manifest.json: {e}");
            }
        }

        match self.get::<Manifest>().await {
            Ok(manifest) => {
                log::warn!("Falling back to manifest.json");
                return Ok(manifest.try_into()?);
            }
            Err(e) => {
                log::debug!("Error reading manifest.json: {e}");
            }
        }

        let cluster = self.get::<Cluster>().await?;
        let manifest_builder = ManifestBuilder::from(cluster);
        let collection_date = self.collection_date().await;
        log::warn!("Falling back to version.json with collection date {collection_date}");
        let manifest = match self {
            Receiver::Elasticsearch(_) => manifest_builder.runner("esdiag").build(),
            _ => manifest_builder.collection_date(collection_date).build(),
        };
        Ok(manifest.try_into()?)
    }
}

impl TryFrom<Uri> for Receiver {
    type Error = eyre::Report;
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
            Receiver::ApiServer(receiver) => write!(f, "REST API {receiver}"),
        }
    }
}
