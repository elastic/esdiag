use super::resolve_archive_path;
use crate::{
    data::diagnostic::{DataSource, data_source::PathType},
    receiver::{Receive, ReceiveMultiple},
};
use bytes::Bytes;
use eyre::{Result, eyre};
use serde::de::DeserializeOwned;
use std::{
    io::{BufReader, Cursor},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::RwLock;
use zip::ZipArchive;

type ArchiveCursor = ZipArchive<BufReader<Cursor<Bytes>>>;
type ArchivePointer = Arc<RwLock<ArchiveCursor>>;

#[derive(Clone)]
pub struct ArchiveBytesReceiver {
    archive: ArchivePointer,
    subdir: Option<PathBuf>,
}

/// A receiver for the Elastic Uploader service (https://upload.elastic.co).
/// This will download the archive on first use and cache it in memory.
impl Receive for ArchiveBytesReceiver {
    async fn collection_date(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }

    async fn is_connected(&self) -> bool {
        true
    }

    /// Read the type's file from the in-memory archive
    async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        let mut archive = self.archive.write().await;

        // Determine the fully-qualified filename within in the archive
        let filename = resolve_archive_path(
            self.subdir.as_ref(),
            &mut *archive,
            T::source(PathType::File)?,
        )?;

        // Read and deserialize the file from the archive
        log::debug!("Reading {}", filename);
        let file = match archive.by_name(&filename) {
            Ok(file) => file,
            Err(_) => return Err(eyre!("Failed to read file ${filename} from archive")),
        };
        let reader = BufReader::new(file);
        let data: T = serde_json::from_reader(reader)?;
        Ok(data)
    }
}

impl ReceiveMultiple for ArchiveBytesReceiver {
    fn set_work_dir(&mut self, work_dir: &str) -> Result<()> {
        log::trace!("Setting subdir: {}", work_dir);
        self.subdir = Some(PathBuf::from(work_dir));
        Ok(())
    }
}

impl TryFrom<Bytes> for ArchiveBytesReceiver {
    type Error = eyre::Report;

    fn try_from(bytes: Bytes) -> Result<Self> {
        log::debug!("Using in-memory archive");
        let archive = ZipArchive::new(BufReader::new(Cursor::new(bytes)))?;
        Ok(Self {
            archive: Arc::new(RwLock::new(archive)),
            subdir: None,
        })
    }
}

impl std::fmt::Display for ArchiveBytesReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Archive Bytes Receiver")
    }
}
