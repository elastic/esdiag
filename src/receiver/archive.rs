use super::{Receive, ReceiveMultiple, ReceiveRaw};
use crate::data::diagnostic::{data_source::PathType, DataSource};
use color_eyre::{eyre::eyre, Result};
use serde::de::DeserializeOwned;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
    sync::Arc,
    time::SystemTime,
};
use tokio::sync::RwLock;
use zip::ZipArchive;

#[derive(Clone)]
pub struct ArchiveReceiver {
    archive: Arc<RwLock<ZipArchive<File>>>,
    filename: String,
    subdir: Option<PathBuf>,
    created_date: SystemTime,
}

impl ArchiveReceiver {
    async fn get_subdir(&self) -> Result<PathBuf> {
        let mut archive = self.archive.write().await;
        let mut path = PathBuf::from(archive.by_index(0)?.name().to_string());
        if path.extension() != None {
            path.pop();
        }
        // Drop known subdirectories, we only want the root directory that
        // contains the diagnostic_manifest.json or manifest.json
        if path.ends_with("cat")
            || path.ends_with("commercial")
            || path.ends_with("docker")
            || path.ends_with("syscalls")
            || path.ends_with("java")
            || path.ends_with("logs")
        {
            path.pop();
        }
        Ok(path)
    }
}

impl TryFrom<PathBuf> for ArchiveReceiver {
    type Error = color_eyre::eyre::Report;

    fn try_from(path: PathBuf) -> Result<Self> {
        let filename = format!("{}", path.display());
        match path.is_file() {
            true => {
                log::debug!("File is valid: {}", path.display());
                let file = File::open(path)?;
                let created_date = file.metadata()?.created()?;
                let archive = ZipArchive::new(file)?;
                Ok(Self {
                    archive: Arc::new(RwLock::new(archive)),
                    created_date,
                    filename,
                    subdir: None,
                })
            }
            false => {
                log::debug!("File is invalid: {}", path.display());
                Err(eyre!("Archive input must be a file: {}", path.display()))
            }
        }
    }
}

impl Receive for ArchiveReceiver {
    async fn collection_date(&self) -> String {
        chrono::DateTime::<chrono::Utc>::from(self.created_date).to_rfc3339()
    }

    async fn is_connected(&self) -> bool {
        let archive = self.archive.read().await;
        let is_empty = archive.is_empty();
        if log::log_enabled!(log::Level::Trace) {
            let file_names: Vec<String> =
                archive.file_names().map(|name| name.to_string()).collect();
            log::trace!("Files in archive: {:?}", file_names);
        }
        log::debug!("Directory {} is valid: {is_empty}", &self.filename);
        is_empty
    }

    /// Read the type's file from the filesystem
    async fn get<T>(&self) -> Result<T>
    where
        T: DeserializeOwned + DataSource,
    {
        let filename = T::source(PathType::File)?;
        let file_str = match &self.subdir {
            // Ugly hack to make ECK bundles with double-slashed paths work
            // This will break if the sub-paths are fixed in the ECK bundles
            Some(subdir) => &format!("{}//{}", subdir.display(), filename),
            None => {
                let subdir = self.get_subdir().await?.join(filename);
                &format!("{}", subdir.display())
            }
        };
        let mut archive = self.archive.write().await;

        // Read lines directly from the compressed file
        log::debug!("Reading {}", file_str);
        let file = archive.by_name(&file_str)?;
        let reader = BufReader::new(file);
        let data: T = serde_json::from_reader(reader)?;
        Ok(data)
    }
}

impl ReceiveRaw for ArchiveReceiver {
    async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        let filename = T::source(PathType::File)?;
        let file_str = match &self.subdir {
            // Ugly hack to make ECK bundles with double-slashed paths work
            // This will break if the sub-paths are fixed in the ECK bundles
            Some(subdir) => &format!("{}//{}", subdir.display(), filename),
            None => {
                let subdir = self.get_subdir().await.map(|s| s.join(filename))?;
                &format!("{}", subdir.display())
            }
        };
        let mut archive = self.archive.write().await;

        // Read lines directly from the compressed file
        log::debug!("Reading {}", file_str);
        let file = archive.by_name(&file_str)?;
        let mut reader = BufReader::new(file);
        let mut data = String::new();
        reader.read_to_string(&mut data)?;
        Ok(data)
    }
}

impl ReceiveMultiple for ArchiveReceiver {
    fn set_work_dir(&mut self, work_dir: &str) -> Result<()> {
        log::trace!("Setting subdir: {}", work_dir);
        self.subdir = Some(PathBuf::from(work_dir));
        Ok(())
    }
}

impl std::fmt::Display for ArchiveReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.filename)
    }
}
