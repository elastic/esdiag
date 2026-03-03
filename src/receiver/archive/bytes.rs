// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::resolve_archive_path;
use crate::{
    processor::diagnostic::data_source::get_source,
    processor::{DataSource, PathType, StreamingDataSource},
    receiver::{Receive, ReceiveMultiple},
};
use bytes::Bytes;
use eyre::{Result, eyre};
use futures::stream::BoxStream;
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

    fn filename(&self) -> Option<String> {
        None
    }

    /// Read the type's file from the in-memory archive
    async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        let mut archive = self.archive.write().await;
        let source_paths = candidate_file_paths::<T>()?;
        let mut last_resolve_error = None;

        for source_path in source_paths {
            match resolve_archive_path(self.subdir.as_ref(), &mut *archive, &source_path) {
                Ok(filename) => {
                    log::debug!("Reading {}", filename);
                    let file = match archive.by_name(&filename) {
                        Ok(file) => file,
                        Err(_) => return Err(eyre!("Failed to read file {filename} from archive")),
                    };
                    let reader = BufReader::new(file);
                    let data: T = serde_json::from_reader(reader)?;
                    return Ok(data);
                }
                Err(e) => {
                    last_resolve_error = Some(e);
                    continue;
                }
            }
        }

        match last_resolve_error {
            Some(e) => Err(e),
            None => Err(eyre!("No candidate source files available for {}", T::name())),
        }
    }

    async fn get_stream<T>(&self) -> Result<BoxStream<'static, Result<T::Item>>>
    where
        T: StreamingDataSource + DeserializeOwned,
        T::Item: DeserializeOwned + Send + 'static,
    {
        super::get_stream_from_archive::<BufReader<Cursor<Bytes>>, T>(
            self.archive.clone(),
            self.subdir.clone(),
        )
        .await
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

fn candidate_file_paths<T: DataSource>() -> Result<Vec<String>> {
    let mut paths = Vec::new();
    paths.push(T::source(PathType::File, None)?);

    for alias in T::aliases() {
        if let Ok((matched_name, source_conf)) = get_source(T::product(), alias, &[]) {
            let path = source_conf.get_file_path(matched_name);
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }

    Ok(paths)
}
