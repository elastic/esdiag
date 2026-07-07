// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::processor::{DataSource, SourceContext, StreamingDataSource};
use super::archive::{normalize_supported_content, normalize_supported_reader_to_temp, supports_json_normalization};
use super::{RawResponse, Receive, ReceiveMultiple, ReceiveRaw};
use eyre::{Result, eyre};
use futures::stream::{self, BoxStream};
use serde::de::DeserializeOwned;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
    sync::Arc,
    sync::OnceLock,
    time::SystemTime,
};
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct DirectoryReceiver {
    path: PathBuf,
    work_dir: String,
    modified_date: SystemTime,
    source_product: Arc<OnceLock<&'static str>>,
    scrubbed: bool,
}

impl TryFrom<PathBuf> for DirectoryReceiver {
    type Error = eyre::Report;

    fn try_from(path: PathBuf) -> Result<Self> {
        match path.is_dir() {
            true => {
                tracing::debug!("Directory is valid: {}", path.display());
                Ok(Self {
                    path: path.clone(),
                    work_dir: String::from(""),
                    modified_date: path.metadata()?.modified()?,
                    source_product: Arc::new(OnceLock::new()),
                    scrubbed: false,
                })
            }
            false => {
                tracing::debug!("Directory is invalid: {}", path.display());
                Err(eyre!("Directory input must be a directory: {}", path.display()))
            }
        }
    }
}

impl Receive for DirectoryReceiver {
    async fn collection_date(&self) -> String {
        chrono::DateTime::<chrono::Utc>::from(self.modified_date).to_rfc3339()
    }

    async fn is_connected(&self) -> bool {
        let is_dir = self.path.is_dir();
        let directory_name = self.path.to_str().unwrap_or("");
        tracing::debug!("Directory {directory_name} is valid: {is_dir}");
        is_dir
    }

    fn filename(&self) -> Option<String> {
        Some(self.path.to_str().unwrap_or("").to_string())
    }

    async fn get<T>(&self) -> Result<T>
    where
        T: DeserializeOwned + DataSource,
    {
        let ctx = self.source_context()?;
        let source_paths = T::candidate_source_file_paths(&ctx)?;
        let mut last_open_error = None;

        for source_path in source_paths {
            let filename = self.path.join(&self.work_dir).join(&source_path);
            tracing::debug!("Reading file: {}", &filename.display());
            match File::open(&filename) {
                Ok(file) => {
                    if should_normalize_file(&source_path, self.scrubbed) {
                        tracing::debug!("Reading {} (scrubbed mode)", source_path);
                        let mut reader = BufReader::new(file);
                        let mut content = String::new();
                        reader.read_to_string(&mut content)?;
                        let content = normalize_file_content_if_needed(&source_path, self.scrubbed, content)?;
                        let data: T = serde_json::from_str(&content)?;
                        return Ok(data);
                    }

                    if self.scrubbed {
                        tracing::debug!("Scrubbed mode read {} (no normalization rules)", source_path);
                    }

                    let reader = BufReader::new(file);
                    let data: T = serde_json::from_reader(reader)?;
                    return Ok(data);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    last_open_error = Some(e);
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }

        match last_open_error {
            Some(e) => Err(e.into()),
            None => Err(eyre!("No candidate source files available for {}", T::name())),
        }
    }

    async fn get_stream<T>(&self) -> Result<BoxStream<'static, Result<T::Item>>>
    where
        T: StreamingDataSource + DeserializeOwned,
        T::Item: DeserializeOwned + Send + 'static,
    {
        let ctx = self.source_context()?;
        let source_path = T::resolve_source_file_path(&ctx)?;
        let filename = self.path.join(&self.work_dir).join(&source_path);
        tracing::debug!("Streaming file: {}", &filename.display());

        let filename_clone = filename.clone();
        let scrubbed = self.scrubbed;
        let source_path_for_scrub = source_path.clone();
        let should_normalize = should_normalize_file(&source_path, scrubbed);
        let (tx, rx) = mpsc::channel(100);

        let tx_err = tx.clone();
        let handle = tokio::task::spawn_blocking(move || match File::open(&filename_clone) {
            Ok(file) => {
                if should_normalize {
                    tracing::debug!("Reading {} (scrubbed mode)", source_path_for_scrub);
                    let reader = BufReader::new(file);
                    match normalize_supported_reader_to_temp(&source_path_for_scrub, reader) {
                        Ok(mut transformed) => {
                            tracing::debug!(
                                "Unscrubbed {} address fields in {}",
                                transformed.transformed_fields,
                                source_path_for_scrub
                            );
                            let reader = BufReader::new(transformed.file.as_file_mut());
                            let mut deserializer = serde_json::Deserializer::from_reader(reader);
                            if let Err(e) = T::deserialize_stream(&mut deserializer, tx.clone()) {
                                tracing::error!("Error deserializing stream: {}", e);
                                let _ = tx.blocking_send(Err(eyre!(e)));
                            }
                        }
                        Err(e) => {
                            let _ = tx.blocking_send(Err(eyre!(e)));
                        }
                    }
                } else {
                    if scrubbed {
                        tracing::debug!("Scrubbed mode read {} (no normalization rules)", source_path_for_scrub);
                    }
                    let reader = BufReader::new(file);
                    let mut deserializer = serde_json::Deserializer::from_reader(reader);
                    if let Err(e) = T::deserialize_stream(&mut deserializer, tx.clone()) {
                        tracing::error!("Error deserializing stream: {}", e);
                        let _ = tx.blocking_send(Err(eyre!(e)));
                    }
                }
            }
            Err(e) => {
                let _ = tx.blocking_send(Err(eyre!(e)));
            }
        });

        tokio::spawn(async move {
            if let Err(e) = handle.await {
                let _ = tx_err.send(Err(eyre!(e))).await;
            }
        });

        Ok(Box::pin(stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        })))
    }
}

impl ReceiveRaw for DirectoryReceiver {
    async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        self.get_raw_response::<T>().await.map(|response| response.body)
    }

    async fn get_raw_response<T>(&self) -> Result<RawResponse>
    where
        T: DataSource,
    {
        let ctx = self.source_context()?;
        let source_paths = T::candidate_source_file_paths(&ctx)?;
        let mut last_open_error = None;

        for source_path in source_paths {
            let filename = self.path.join(&self.work_dir).join(&source_path);
            tracing::debug!("Reading file: {}", &filename.display());
            match File::open(&filename) {
                Ok(file) => {
                    if self.scrubbed {
                        tracing::debug!("Reading {} (scrubbed mode)", source_path);
                    }
                    let mut reader = BufReader::new(file);
                    let mut data = String::new();
                    reader.read_to_string(&mut data)?;
                    let body = normalize_file_content_if_needed(&source_path, self.scrubbed, data)?;
                    let response_size_bytes = body.len() as u64;
                    return Ok(RawResponse {
                        body,
                        status: None,
                        response_time_ms: 0,
                        response_size_bytes,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    last_open_error = Some(e);
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }

        match last_open_error {
            Some(e) => Err(e.into()),
            None => Err(eyre!("No candidate source files available for {}", T::name())),
        }
    }
}

impl ReceiveMultiple for DirectoryReceiver {
    fn set_work_dir(&mut self, work_dir: &str) -> Result<()> {
        self.work_dir = String::from(work_dir);
        Ok(())
    }
}

impl std::fmt::Display for DirectoryReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Directory {}", self.path.display())
    }
}

impl DirectoryReceiver {
    pub(crate) fn clone_for_subdir(&self, work_dir: &str) -> Self {
        Self {
            path: self.path.clone(),
            work_dir: work_dir.to_string(),
            modified_date: self.modified_date,
            source_product: Arc::new(OnceLock::new()),
            scrubbed: self.scrubbed,
        }
    }

    pub fn set_scrubbed(&mut self, scrubbed: bool) {
        self.scrubbed = scrubbed;
    }

    pub async fn read_bundle_json<T>(&self, filename: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let path = self.path.join(&self.work_dir).join(filename);
        tracing::debug!("Reading bundle file: {}", path.display());
        let file = File::open(path)?;
        if !should_normalize_file(filename, self.scrubbed) {
            if self.scrubbed {
                tracing::debug!("Scrubbed mode read {} (no normalization rules)", filename);
            }
            let reader = BufReader::new(file);
            return serde_json::from_reader(reader).map_err(Into::into);
        }

        let mut reader = BufReader::new(file);
        let mut content = String::new();
        reader.read_to_string(&mut content)?;
        let content = normalize_file_content_if_needed(filename, self.scrubbed, content)?;
        serde_json::from_str(&content).map_err(Into::into)
    }

    pub fn set_source_product(&self, product: &'static str) -> Result<()> {
        match self.source_product.get() {
            Some(existing) if *existing != product => Err(eyre!(
                "Directory receiver source product already set to {}, cannot change to {}",
                existing,
                product
            )),
            Some(_) => Ok(()),
            None => self
                .source_product
                .set(product)
                .map_err(|_| eyre!("Failed to initialize directory receiver source product")),
        }
    }

    pub fn source_product(&self) -> Result<&'static str> {
        self.source_product
            .get()
            .copied()
            .ok_or_else(|| eyre!("Directory receiver source product is not initialized"))
    }

    pub fn source_context(&self) -> Result<SourceContext> {
        Ok(SourceContext::new(self.source_product()?, None))
    }
}

fn normalize_file_content_if_needed(logical_name: &str, scrubbed: bool, content: String) -> Result<String> {
    if !scrubbed {
        return Ok(content);
    }

    if !supports_json_normalization(logical_name) {
        tracing::debug!("Scrubbed mode read {} (no normalization rules)", logical_name);
        return Ok(content);
    }

    let transformed = normalize_supported_content(logical_name, content)?;
    tracing::debug!(
        "Unscrubbed {} address fields in {}",
        transformed.transformed_fields,
        logical_name
    );
    Ok(transformed.content)
}

fn should_normalize_file(logical_name: &str, scrubbed: bool) -> bool {
    scrubbed && supports_json_normalization(logical_name)
}
