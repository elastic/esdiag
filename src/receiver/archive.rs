use super::Receive;
use crate::data::{diagnostic::DataSource, Uri};
use color_eyre::{eyre::eyre, Result};
use serde::de::DeserializeOwned;
use std::{fs::File, io::BufReader, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;
use zip::ZipArchive;

#[derive(Clone)]
pub struct ArchiveReceiver {
    archive: Arc<RwLock<ZipArchive<File>>>,
    subdir: Option<PathBuf>,
    uri: Uri,
}

impl ArchiveReceiver {
    async fn get_subdir(&self) -> Result<PathBuf> {
        let mut archive = self.archive.write().await;
        let mut path = PathBuf::from(archive.by_index(0)?.name().to_string());
        if path.extension() != None {
            path.pop();
        }
        Ok(path)
    }
}

impl TryFrom<Uri> for ArchiveReceiver {
    type Error = color_eyre::eyre::Report;

    /// Create a new ArchiveReceiver from a Uri
    fn try_from(uri: Uri) -> Result<Self> {
        match uri {
            Uri::File(ref path) => match path.is_file() {
                true => {
                    log::debug!("File is valid: {}", path.display());
                    Ok(Self {
                        archive: Arc::new(RwLock::new(ZipArchive::new(File::open(path)?)?)),
                        uri,
                        subdir: None,
                    })
                }
                false => {
                    log::debug!("File is invalid: {}", path.display());
                    Err(eyre!("Archive input must be a file: {}", path.display()))
                }
            },
            _ => Err(eyre!("Input must be a file")),
        }
    }
}

impl Receive for ArchiveReceiver {
    async fn is_connected(&self) -> bool {
        let archive = self.archive.read().await;
        let is_empty = archive.is_empty();
        if log::log_enabled!(log::Level::Trace) {
            let file_names: Vec<String> =
                archive.file_names().map(|name| name.to_string()).collect();
            log::trace!("Files in archive: {:?}", file_names);
        }
        let filename = self.uri.to_string();
        log::debug!("Directory {filename} is valid: {is_empty}");
        is_empty
    }

    /// Read the type's file from the filesystem
    async fn get<T>(&self) -> Result<T>
    where
        T: DeserializeOwned + DataSource,
    {
        let filename = T::source(&self.uri)?;
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
        let reader = BufReader::new(file);
        let data: T = serde_json::from_reader(reader)?;
        Ok(data)
    }

    fn set_work_dir(&mut self, work_dir: &str) -> Result<()> {
        log::trace!("Setting subdir: {}", work_dir);
        self.subdir = Some(PathBuf::from(work_dir));
        Ok(())
    }
}

impl std::fmt::Display for ArchiveReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.uri)
    }
}
