use super::Receive;
use crate::data::diagnostic::{data_source::PathType, DataSource};
use bytes::Bytes;
use color_eyre::eyre::{eyre, Result};
use serde::de::DeserializeOwned;
use std::{
    io::{BufReader, Cursor},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::RwLock;
use url::Url;
use zip::ZipArchive;

type ArchiveCursor = ZipArchive<BufReader<Cursor<Bytes>>>;
type ArchivePointer = Arc<RwLock<Option<ArchiveCursor>>>;

#[derive(Clone)]
pub struct ElasticUploaderReceiver {
    url: Url,
    token: String,
    archive: ArchivePointer,
}

impl Receive for ElasticUploaderReceiver {
    async fn is_connected(&self) -> bool {
        let client = reqwest::Client::new();
        let request = client.head(self.url.clone());
        let request = request.header("Authorization", format!("Bearer {}", self.token));

        match request.send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        // We are using interior mutability here to cache the archive to memory only once
        let mut archive_lock = self.archive.write().await;
        if archive_lock.is_none() {
            let file = download_file(self.url.clone(), &self.token).await?;
            archive_lock.replace(file);
        }

        let filename = T::source(PathType::File)?;

        let data: T = if let Some(archive) = archive_lock.as_mut() {
            let mut file_str = PathBuf::from(archive.by_index(0)?.name().to_string());
            if file_str.extension() != None {
                file_str.pop();
            }
            let file_str = file_str.join(filename).display().to_string();

            // Read lines directly from the compressed file
            log::debug!("Reading {}", file_str);
            let file = archive.by_name(&file_str)?;
            let reader = BufReader::new(file);
            serde_json::from_reader(reader)?
        } else {
            return Err(eyre!("Archive is not inialized"));
        };
        Ok(data)
    }
}

impl TryFrom<Url> for ElasticUploaderReceiver {
    type Error = color_eyre::eyre::Report;

    fn try_from(url: Url) -> Result<Self> {
        let mut url = url.clone();
        let token = url
            .password()
            .ok_or_else(|| eyre!("No token provided"))?
            .to_string();
        url.set_username("").ok();
        url.set_password(None).ok();
        log::info!("Downloading archive from {url}");
        Ok(Self {
            url,
            token,
            archive: Arc::new(RwLock::new(None)),
        })
    }
}

async fn download_file(url: Url, token: &String) -> Result<ArchiveCursor> {
    let client = reqwest::Client::new();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Authorization",
        reqwest::header::HeaderValue::from_str(&token)?,
    );
    let request = client.get(url.clone()).headers(headers);
    let response = request.send().await?;
    let bytes = response.bytes().await?;
    log::debug!("Archive size: {} bytes", bytes.len());
    let cursor = BufReader::new(Cursor::new(bytes));
    let archive = ZipArchive::new(cursor)?;
    Ok(archive)
}

impl std::fmt::Display for ElasticUploaderReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}
