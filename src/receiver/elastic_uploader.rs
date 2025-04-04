use super::{archive::trim_to_working_directory, Receive, ReceiveMultiple};
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
    archive: ArchivePointer,
    subdir: Option<PathBuf>,
    token: String,
    url: Url,
}

impl ElasticUploaderReceiver {
    fn resolve_archive_path(&self, archive: &mut ArchiveCursor, filename: &str) -> Result<String> {
        let full_path = match &self.subdir {
            // Ugly hack to make ECK bundles with double-slashed paths work
            // This will break if the sub-paths are fixed in the ECK bundles
            Some(subdir) => format!("{}//{}", subdir.display(), filename),
            None => {
                let mut path = PathBuf::from(archive.by_index(0)?.name().to_string());
                trim_to_working_directory(&mut path);
                let path = path.join(filename);
                format!("{}", path.display())
            }
        };
        Ok(full_path)
    }
}

/// A receiver for the Elastic Uploader service (https://upload.elastic.co).
/// This will download the archive on first use and cache it in memory.
impl Receive for ElasticUploaderReceiver {
    async fn collection_date(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }

    async fn is_connected(&self) -> bool {
        let client = reqwest::Client::new();
        let request = client.head(self.url.clone());
        let request = request.header("Authorization", format!("Bearer {}", self.token));

        match request.send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    /// Read the type's file from the in-memory archive
    async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        let mut archive_lock = self.archive.write().await;
        // We are using interior mutability here to cache the archive to memory on first access
        if archive_lock.is_none() {
            let archive = get_file_from_uploader(self.url.clone(), &self.token).await?;
            archive_lock.replace(archive);
        }

        // Early return if archive is not available
        let Some(archive) = archive_lock.as_mut() else {
            return Err(eyre!("Archive was not downloaded or cached"));
        };

        // Determine the fully-qualified filename within in the archive
        let filename = self.resolve_archive_path(archive, T::source(PathType::File)?)?;

        // Read and deserialize the file from the archive
        log::debug!("Reading {}", filename);
        let file = archive.by_name(&filename)?;
        let reader = BufReader::new(file);
        let data: T = serde_json::from_reader(reader)?;
        Ok(data)
    }
}

impl ReceiveMultiple for ElasticUploaderReceiver {
    fn set_work_dir(&mut self, work_dir: &str) -> Result<()> {
        log::trace!("Setting subdir: {}", work_dir);
        self.subdir = Some(PathBuf::from(work_dir));
        Ok(())
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
        // Since token authentication is by header, clear provided username and password from the URL
        url.set_username("").ok();
        url.set_password(None).ok();
        log::info!("Downloading archive from {url}");
        Ok(Self {
            archive: Arc::new(RwLock::new(None)),
            subdir: None,
            token,
            url,
        })
    }
}

/// Downloads a file from the Elastic Uploader service given a URL and token
/// The URL format of `https://upload.elastic.co/...` will have been validated previously.
async fn get_file_from_uploader(url: Url, token: &String) -> Result<ArchiveCursor> {
    let client = reqwest::Client::new();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Authorization",
        reqwest::header::HeaderValue::from_str(&token)?,
    );
    let request = client.get(url.clone()).headers(headers);
    let response = request.send().await?;
    let bytes = response.bytes().await?;
    log::debug!("Downloaded archive size: {} bytes", bytes.len());
    let cursor = BufReader::new(Cursor::new(bytes));
    Ok(ZipArchive::new(cursor)?)
}

impl std::fmt::Display for ElasticUploaderReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}
