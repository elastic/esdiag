use super::{Receive, ReceiveMultiple, ReceiveRaw};
use crate::data::diagnostic::{DataSource, data_source::PathType};
use eyre::{Result, eyre};
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

pub fn trim_to_working_directory(path: &mut PathBuf) {
    // Drop any filename
    if path.extension() != None {
        path.pop();
    }
    // Drop any known subdirectories
    if path.ends_with("cat")
        || path.ends_with("commercial")
        || path.ends_with("docker")
        || path.ends_with("syscalls")
        || path.ends_with("java")
        || path.ends_with("logs")
    {
        path.pop();
    }
}

#[derive(Clone)]
pub struct ArchiveReceiver {
    archive: Arc<RwLock<ZipArchive<File>>>,
    filename: String,
    subdir: Option<PathBuf>,
    modified_date: SystemTime,
}

impl ArchiveReceiver {
    fn resolve_archive_path(
        &self,
        archive: &mut ZipArchive<File>,
        filename: &str,
    ) -> Result<String> {
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

impl TryFrom<PathBuf> for ArchiveReceiver {
    type Error = eyre::Report;

    fn try_from(path: PathBuf) -> Result<Self> {
        let filename = format!("{}", path.display());
        match path.is_file() {
            true => {
                log::debug!("File is valid: {}", path.display());
                let file = File::open(path)?;
                let modified_date = file.metadata()?.modified()?;
                let archive = ZipArchive::new(file)?;
                Ok(Self {
                    archive: Arc::new(RwLock::new(archive)),
                    modified_date,
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
        chrono::DateTime::<chrono::Utc>::from(self.modified_date).to_rfc3339()
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
        let mut archive = self.archive.write().await;

        // Determine the fully-qualified filename within the archive
        let filename = self.resolve_archive_path(&mut *archive, T::source(PathType::File)?)?;

        // Read lines directly from the compressed file
        log::debug!("Reading {}", filename);
        let file = archive.by_name(&filename)?;
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
        let mut archive = self.archive.write().await;

        // Determine the fully-qualified filename within the archive
        let filename = self.resolve_archive_path(&mut *archive, T::source(PathType::File)?)?;

        // Read lines directly from the compressed file
        log::debug!("Reading {}", filename);
        let file = archive.by_name(&filename)?;
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
