// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::resolve_archive_path;
use crate::{
    processor::{DataSource, SourceContext, StreamingDataSource},
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
    sync::OnceLock,
};
use tokio::sync::RwLock;
use zip::ZipArchive;

type ArchiveCursor = ZipArchive<BufReader<Cursor<Bytes>>>;
type ArchivePointer = Arc<RwLock<ArchiveCursor>>;

#[derive(Clone)]
pub struct ArchiveBytesReceiver {
    archive: ArchivePointer,
    subdir: Option<PathBuf>,
    source_product: Arc<OnceLock<&'static str>>,
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
        let ctx = self.source_context()?;
        let source_paths = T::candidate_source_file_paths(&ctx)?;
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
                    let data: T = if filename.ends_with(".yaml") || filename.ends_with(".yml") {
                        serde_yaml::from_reader(reader)?
                    } else {
                        serde_json::from_reader(reader)?
                    };
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
            None => Err(eyre!(
                "No candidate source files available for {}",
                T::name()
            )),
        }
    }

    async fn get_stream<T>(&self) -> Result<BoxStream<'static, Result<T::Item>>>
    where
        T: StreamingDataSource + DeserializeOwned,
        T::Item: DeserializeOwned + Send + 'static,
    {
        let ctx = self.source_context()?;
        super::get_stream_from_archive::<BufReader<Cursor<Bytes>>, T>(
            self.archive.clone(),
            self.subdir.clone(),
            ctx,
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
            source_product: Arc::new(OnceLock::new()),
        })
    }
}

impl std::fmt::Display for ArchiveBytesReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Archive Bytes Receiver")
    }
}

impl ArchiveBytesReceiver {
    pub async fn read_bundle_json<T>(&self, filename: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let mut archive = self.archive.write().await;
        let filename = resolve_archive_path(self.subdir.as_ref(), &mut *archive, filename)?;
        log::debug!("Reading bundle file {}", filename);
        let file = match archive.by_name(&filename) {
            Ok(file) => file,
            Err(_) => return Err(eyre!("Failed to read file {filename} from archive")),
        };
        let reader = BufReader::new(file);
        serde_json::from_reader(reader).map_err(Into::into)
    }

    pub async fn read_bundle_yaml<T>(&self, filename: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let mut archive = self.archive.write().await;
        let filename = resolve_archive_path(self.subdir.as_ref(), &mut *archive, filename)?;
        log::debug!("Reading bundle YAML file {}", filename);
        let file = match archive.by_name(&filename) {
            Ok(file) => file,
            Err(_) => return Err(eyre!("Failed to read file {filename} from archive")),
        };
        let reader = BufReader::new(file);
        serde_yaml::from_reader(reader).map_err(Into::into)
    }

    pub fn set_source_product(&self, product: &'static str) -> Result<()> {
        match self.source_product.get() {
            Some(existing) if *existing != product => Err(eyre!(
                "Archive receiver source product already set to {}, cannot change to {}",
                existing,
                product
            )),
            Some(_) => Ok(()),
            None => self
                .source_product
                .set(product)
                .map_err(|_| eyre!("Failed to initialize archive receiver source product")),
        }
    }

    pub fn source_product(&self) -> Result<&'static str> {
        self.source_product
            .get()
            .copied()
            .ok_or_else(|| eyre!("Archive receiver source product is not initialized"))
    }

    pub fn source_context(&self) -> Result<SourceContext> {
        Ok(SourceContext::new(self.source_product()?, None))
    }
}
