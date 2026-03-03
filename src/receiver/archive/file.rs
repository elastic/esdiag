// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::resolve_archive_path;
use crate::{
    processor::diagnostic::data_source::get_source,
    processor::{DataSource, PathType, StreamingDataSource},
    receiver::{Receive, ReceiveMultiple, ReceiveRaw},
};
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
use tokio::sync::RwLock;
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
        let source_paths = candidate_file_paths::<T>()?;
        let mut last_resolve_error = None;

        for source_path in source_paths {
            match resolve_archive_path(self.subdir.as_ref(), &mut *archive, &source_path) {
                Ok(filename) => {
                    log::debug!("Reading {}", filename);
                    let file = archive.by_name(&filename)?;
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
        super::get_stream_from_archive::<File, T>(self.archive.clone(), self.subdir.clone()).await
    }
}

impl ReceiveRaw for ArchiveFileReceiver {
    async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        let mut archive = self.archive.write().await;
        let source_paths = candidate_file_paths::<T>()?;
        let mut last_resolve_error = None;

        for source_path in source_paths {
            match resolve_archive_path(self.subdir.as_ref(), &mut *archive, &source_path) {
                Ok(filename) => {
                    log::debug!("Reading {}", filename);
                    let file = archive.by_name(&filename)?;
                    let mut reader = BufReader::new(file);
                    let mut data = String::new();
                    reader.read_to_string(&mut data)?;
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
