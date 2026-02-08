// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::resolve_archive_path;
use crate::{
    processor::{DataSource, PathType, StreamingDataSource},
    receiver::{Receive, ReceiveMultiple, ReceiveRaw},
};
use async_stream::stream;
use eyre::{Result, eyre};
use futures::stream::BoxStream;
use serde::de::DeserializeOwned;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
    sync::Arc,
    time::SystemTime,
};
use tokio::sync::{RwLock, mpsc};
use zip::ZipArchive;

#[derive(Clone)]
pub struct ArchiveFileReceiver {
    archive: Arc<RwLock<ZipArchive<File>>>,
    filename: String,
    subdir: Option<PathBuf>,
    modified_date: SystemTime,
}

impl TryFrom<PathBuf> for ArchiveFileReceiver {
    type Error = eyre::Report;

    fn try_from(path: PathBuf) -> Result<Self> {
        let filename = format!("{}", path.file_name().unwrap_or_default().display());
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

impl Receive for ArchiveFileReceiver {
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

    fn filename(&self) -> Option<String> {
        Some(self.filename.clone())
    }

    /// Read the type's file from the filesystem
    async fn get<T>(&self) -> Result<T>
    where
        T: DeserializeOwned + DataSource,
    {
        let mut archive = self.archive.write().await;

        // Determine the fully-qualified filename within the archive
        let filename = resolve_archive_path(
            self.subdir.as_ref(),
            &mut *archive,
            T::source(PathType::File)?,
        )?;

        // Read lines directly from the compressed file
        log::debug!("Reading {}", filename);
        let file = archive.by_name(&filename)?;
        let reader = BufReader::new(file);
        let data: T = serde_json::from_reader(reader)?;
        Ok(data)
    }

    async fn get_stream<T>(&self) -> Result<BoxStream<'static, Result<T::Item>>>
    where
        T: StreamingDataSource + DeserializeOwned,
        T::Item: DeserializeOwned + Send + 'static,
    {
        let archive = self.archive.clone();
        let subdir = self.subdir.clone();
        let (tx, mut rx) = mpsc::channel(100);

        tokio::task::spawn_blocking(move || {
            // This blocks other readers/writers of the archive for the duration of this file's processing
            let mut archive_guard = archive.blocking_write();
            let filename = match resolve_archive_path(
                subdir.as_ref(),
                &mut *archive_guard,
                match T::source(PathType::File) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = tx.blocking_send(Err(eyre!(e)));
                        return;
                    }
                },
            ) {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx.blocking_send(Err(eyre!(e)));
                    return;
                }
            };

            log::debug!("Streaming from archive: {}", filename);
            match archive_guard.by_name(&filename) {
                Ok(file) => {
                    let reader = BufReader::new(file);
                    let mut deserializer = serde_json::Deserializer::from_reader(reader);
                    if let Err(e) = T::deserialize_stream(&mut deserializer, tx.clone()) {
                        log::error!("Error deserializing stream from archive: {}", e);
                        let _ = tx.blocking_send(Err(eyre!(e)));
                    }
                }
                Err(e) => {
                    let _ = tx.blocking_send(Err(eyre!(e)));
                }
            }
        });

        Ok(Box::pin(stream! {
            while let Some(item) = rx.recv().await {
                yield item;
            }
        }))
    }
}

impl ReceiveRaw for ArchiveFileReceiver {
    async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        let mut archive = self.archive.write().await;

        // Determine the fully-qualified filename within in the archive
        let filename = resolve_archive_path(
            self.subdir.as_ref(),
            &mut *archive,
            T::source(PathType::File)?,
        )?;

        // Read lines directly from the compressed file
        log::debug!("Reading {}", filename);
        let file = archive.by_name(&filename)?;
        let mut reader = BufReader::new(file);
        let mut data = String::new();
        reader.read_to_string(&mut data)?;
        Ok(data)
    }
}

impl ReceiveMultiple for ArchiveFileReceiver {
    fn set_work_dir(&mut self, work_dir: &str) -> Result<()> {
        log::trace!("Setting subdir: {}", work_dir);
        self.subdir = Some(PathBuf::from(work_dir));
        Ok(())
    }
}

impl std::fmt::Display for ArchiveFileReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.filename)
    }
}
