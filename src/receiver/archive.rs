// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::processor::{DataSource, SourceContext, StreamingDataSource};
use eyre::Result;
use futures::stream::{self, BoxStream};
use serde::de::DeserializeOwned;
use std::{
    io::{BufReader, Read, Seek},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::{RwLock, mpsc};
use zip::ZipArchive;

mod bytes;
mod file;

pub use bytes::*;
pub use file::*;

pub async fn get_stream_from_archive<R, T>(
    archive: Arc<RwLock<ZipArchive<R>>>,
    subdir: Option<PathBuf>,
    ctx: SourceContext,
) -> Result<BoxStream<'static, Result<T::Item>>>
where
    R: Read + Seek + Send + Sync + 'static,
    T: StreamingDataSource + DeserializeOwned + DataSource,
    T::Item: DeserializeOwned + Send + 'static,
{
    let (tx, rx) = mpsc::channel(100);

    let tx_err = tx.clone();
    let handle = tokio::task::spawn_blocking(move || {
        let mut archive_guard = archive.blocking_write();
        let source_path = match T::resolve_source_file_path(&ctx) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.blocking_send(Err(eyre::eyre!(e)));
                return;
            }
        };
        let filename =
            match resolve_archive_path(subdir.as_ref(), &mut *archive_guard, &source_path) {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx.blocking_send(Err(eyre::eyre!(e)));
                    return;
                }
            };

        tracing::debug!("Streaming from archive: {}", filename);
        let stream_result = match archive_guard.by_name(&filename) {
            Ok(file) => {
                let reader = BufReader::new(file);
                let mut deserializer = serde_json::Deserializer::from_reader(reader);
                T::deserialize_stream(&mut deserializer, tx.clone())
                    .map_err(|e| eyre::eyre!(e.to_string()))
            }
            Err(e) => Err(eyre::eyre!(e)),
        };

        if let Err(e) = stream_result {
            tracing::error!("Error deserializing stream from archive: {}", e);
            let _ = tx.blocking_send(Err(e));
        }
    });

    tokio::spawn(async move {
        if let Err(e) = handle.await
            && e.is_panic()
        {
            let _ = tx_err
                .send(Err(eyre::eyre!("Streaming task panicked")))
                .await;
        }
    });

    Ok(Box::pin(stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    })))
}

pub fn trim_to_working_directory(path: &mut PathBuf) {
    // Drop any filename
    if path.extension().is_some() {
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

pub fn resolve_archive_path<A: Read + Seek>(
    subdir: Option<&PathBuf>,
    archive: &mut ZipArchive<A>,
    filename: &str,
) -> Result<String> {
    fn normalize_archive_separators(path: &str) -> String {
        path.replace('\\', "/")
    }

    let path = if let Some(dir) = subdir {
        let mut workdir = PathBuf::from(archive.by_index(0)?.name().to_string());
        trim_to_working_directory(&mut workdir);
        let path = normalize_archive_separators(
            workdir.join(dir).join(filename).to_string_lossy().as_ref(),
        );
        if archive.by_name(&path).is_ok() {
            return Ok(path);
        } else {
            // Fall back to double slash for ECK bundles with faulty paths
            let base = normalize_archive_separators(workdir.join(dir).to_string_lossy().as_ref());
            format!("{base}//{filename}")
        }
    } else {
        let mut path = PathBuf::from(archive.by_index(0)?.name().to_string());
        trim_to_working_directory(&mut path);
        normalize_archive_separators(path.join(filename).to_string_lossy().as_ref())
    };

    if archive.by_name(&path).is_ok() {
        Ok(path)
    } else {
        Err(eyre::eyre!("File not found in archive: {}", path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn create_test_archive_with_double_slash() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));

            zip.start_file("version.json", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(br#"{"version": "2.4.0"}"#).unwrap();

            // Add a file with double slash path (legacy ECK bundle format)
            zip.start_file(
                "namespace/elasticsearch/cluster-one//version.json",
                SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(br#"{"version": "9.0.0"}"#).unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    fn create_test_archive_with_single_slash() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            zip.start_file("eck-diagnostics/version.json", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(br#"{"version": "3.0.0"}"#).unwrap();

            // Add a file with single slash path (standard format)
            zip.start_file(
                "eck-diagnostics/namespace/elasticsearch/cluster-two/version.json",
                SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(br#"{"version": "8.1.0"}"#).unwrap();

            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn resolve_archive_path_with_double_slash_returns_ok() {
        let archive_data = create_test_archive_with_double_slash();
        let mut archive = ZipArchive::new(Cursor::new(archive_data)).unwrap();

        let subdir = PathBuf::from("namespace/elasticsearch/cluster-one");
        let result = resolve_archive_path(Some(&subdir), &mut archive, "version.json").unwrap();

        assert_eq!(result, "namespace/elasticsearch/cluster-one//version.json");
    }

    #[test]
    fn resolve_archive_path_with_single_slash_returns_ok() {
        let archive_data = create_test_archive_with_single_slash();
        let mut archive = ZipArchive::new(Cursor::new(archive_data)).unwrap();

        let subdir = PathBuf::from("namespace/elasticsearch/cluster-two");
        let result = resolve_archive_path(Some(&subdir), &mut archive, "version.json").unwrap();

        assert_eq!(
            result,
            "eck-diagnostics/namespace/elasticsearch/cluster-two/version.json"
        );
    }

    #[test]
    fn resolve_archive_path_with_windows_style_subdir_returns_ok() {
        let archive_data = create_test_archive_with_single_slash();
        let mut archive = ZipArchive::new(Cursor::new(archive_data)).unwrap();

        // Simulates subdir values built on Windows.
        let subdir = PathBuf::from(r"namespace\elasticsearch\cluster-two");
        let result = resolve_archive_path(Some(&subdir), &mut archive, "version.json").unwrap();

        assert_eq!(
            result,
            "eck-diagnostics/namespace/elasticsearch/cluster-two/version.json"
        );
    }

    #[test]
    fn archive_path_without_subdir_returns_ok() {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));

            // Add a root level file
            zip.start_file("file.json", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(br#"{"test": true}"#).unwrap();

            zip.finish().unwrap();
        }

        let mut archive = ZipArchive::new(Cursor::new(buf)).unwrap();
        let result = resolve_archive_path(None, &mut archive, "file.json").unwrap();

        // Should derive path from first entry in archive
        assert_eq!(result, "file.json");
    }

    #[test]
    fn missing_file_returns_err() {
        let archive_data = create_test_archive_with_single_slash();
        let mut archive = ZipArchive::new(Cursor::new(archive_data)).unwrap();
        let result = resolve_archive_path(None, &mut archive, "missing.json");
        assert!(result.unwrap_err().to_string().contains("File not found"));
    }

    #[test]
    fn trim_removes_files_and_known_subdirectories() {
        // Test with filename
        let mut path = PathBuf::from("root/nested/file.txt");
        trim_to_working_directory(&mut path);
        assert_eq!(path, PathBuf::from("root/nested"));

        // Test with known subdirectories
        let mut path = PathBuf::from("root/cat");
        trim_to_working_directory(&mut path);
        assert_eq!(path, PathBuf::from("root"));

        let mut path = PathBuf::from("root/logs");
        trim_to_working_directory(&mut path);
        assert_eq!(path, PathBuf::from("root"));

        let mut path = PathBuf::from("root/docker");
        trim_to_working_directory(&mut path);
        assert_eq!(path, PathBuf::from("root"));
    }
}
