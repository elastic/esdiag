// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::processor::{DataSource, PathType, StreamingDataSource};
use super::{Receive, ReceiveMultiple, ReceiveRaw};
use crate::processor::diagnostic::data_source::get_source;
use eyre::{Result, eyre};
use futures::stream::{self, BoxStream};
use serde::de::DeserializeOwned;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
    time::SystemTime,
};
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct DirectoryReceiver {
    path: PathBuf,
    work_dir: String,
    modified_date: SystemTime,
}

impl TryFrom<PathBuf> for DirectoryReceiver {
    type Error = eyre::Report;

    fn try_from(path: PathBuf) -> Result<Self> {
        match path.is_dir() {
            true => {
                log::debug!("Directory is valid: {}", path.display());
                Ok(Self {
                    path: path.clone(),
                    work_dir: String::from(""),
                    modified_date: path.metadata()?.modified()?,
                })
            }
            false => {
                log::debug!("Directory is invalid: {}", path.display());
                Err(eyre!(
                    "Directory input must be a directory: {}",
                    path.display()
                ))
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
        log::debug!("Directory {directory_name} is valid: {is_dir}");
        is_dir
    }

    fn filename(&self) -> Option<String> {
        Some(self.path.to_str().unwrap_or("").to_string())
    }

    async fn get<T>(&self) -> Result<T>
    where
        T: DeserializeOwned + DataSource,
    {
        let source_paths = candidate_file_paths::<T>()?;
        let mut last_open_error = None;

        for source_path in source_paths {
            let filename = self.path.join(&self.work_dir).join(source_path);
            log::debug!("Reading file: {}", &filename.display());
            match File::open(&filename) {
                Ok(file) => {
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
        let filename = self
            .path
            .join(&self.work_dir)
            .join(T::source(PathType::File, None)?);
        log::debug!("Streaming file: {}", &filename.display());

        let filename_clone = filename.clone();
        let (tx, rx) = mpsc::channel(100);

        let tx_err = tx.clone();
        let handle = tokio::task::spawn_blocking(move || match File::open(&filename_clone) {
            Ok(file) => {
                let reader = BufReader::new(file);
                let mut deserializer = serde_json::Deserializer::from_reader(reader);
                if let Err(e) = T::deserialize_stream(&mut deserializer, tx.clone()) {
                    log::error!("Error deserializing stream: {}", e);
                    let _ = tx.blocking_send(Err(eyre!(e)));
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
        let source_paths = candidate_file_paths::<T>()?;
        let mut last_open_error = None;

        for source_path in source_paths {
            let filename = self.path.join(&self.work_dir).join(source_path);
            log::debug!("Reading file: {}", &filename.display());
            match File::open(&filename) {
                Ok(file) => {
                    let mut reader = BufReader::new(file);
                    let mut data = String::new();
                    reader.read_to_string(&mut data)?;
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
            None => Err(eyre!(
                "No candidate source files available for {}",
                T::name()
            )),
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
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Directory {}", self.path.display())
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
