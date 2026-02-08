// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::resolve_archive_path;
use crate::{
    processor::{DataSource, PathType, StreamingDataSource},
    receiver::{Receive, ReceiveMultiple},
};
use async_stream::stream;
use bytes::Bytes;
use eyre::{Result, eyre};
use futures::stream::BoxStream;
use serde::de::DeserializeOwned;
use std::{
    io::{BufReader, Cursor},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::{RwLock, mpsc};
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

    fn filename(&self) -> Option<String> {
        None
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

    async fn get_stream<T>(&self) -> Result<BoxStream<'static, Result<T::Item>>>
    where
        T: StreamingDataSource + DeserializeOwned,
        T::Item: DeserializeOwned + Send + 'static,
    {
        let archive = self.archive.clone();
        let subdir = self.subdir.clone();
        let (tx, mut rx) = mpsc::channel(100);

        tokio::task::spawn_blocking(move || {
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

            log::debug!("Streaming from in-memory archive: {}", filename);
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
