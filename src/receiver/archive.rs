// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::processor::{DataSource, SourceContext, StreamingDataSource};
use crate::receiver::MissingSource;
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
        let filename = match resolve_archive_path(subdir.as_ref(), &mut *archive_guard, &source_path) {
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
                T::deserialize_stream(&mut deserializer, tx.clone()).map_err(|e| eyre::eyre!(e.to_string()))
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
            let _ = tx_err.send(Err(eyre::eyre!("Streaming task panicked"))).await;
        }
    });

    Ok(Box::pin(stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    })))
}

/// Stream the lines of one archive entry. The archive stays locked while the
/// stream is being read, so consumers must drain it before issuing other reads
/// on the same receiver.
pub async fn read_lines_from_archive<R>(
    archive: Arc<RwLock<ZipArchive<R>>>,
    subdir: Option<PathBuf>,
    filename: &str,
) -> Result<BoxStream<'static, Result<String>>>
where
    R: Read + Seek + Send + Sync + 'static,
{
    let path = {
        let mut guard = archive.write().await;
        resolve_archive_path(subdir.as_ref(), &mut *guard, filename)?
    };
    tracing::debug!("Streaming lines from archive: {}", path);
    let (tx, rx) = mpsc::channel(256);
    let producer_tx = tx.clone();
    let handle = tokio::task::spawn_blocking(move || {
        let mut guard = archive.blocking_write();
        match guard.by_name(&path) {
            Ok(file) => crate::receiver::send_bundle_lines(BufReader::new(file), &producer_tx),
            Err(err) => {
                let _ = producer_tx.blocking_send(Err(eyre::eyre!(err)));
            }
        }
    });
    Ok(crate::receiver::bundle_line_stream(rx, handle, tx))
}

fn path_components(path: &str) -> Vec<String> {
    path.split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .map(str::to_string)
        .collect()
}

/// Directory names directly beneath `dir`, resolved against the same bundle
/// root and working subdirectory as [`resolve_archive_path`]. Empty path
/// components are ignored so ECK's `name//file` entries list correctly.
pub fn list_archive_dirs<A: Read + Seek>(
    subdir: Option<&PathBuf>,
    archive: &mut ZipArchive<A>,
    dir: &str,
) -> Result<Vec<String>> {
    let mut root = PathBuf::from(archive.by_index(0)?.name().to_string());
    trim_to_working_directory(&mut root);
    let mut prefix = path_components(&root.to_string_lossy());
    if let Some(subdir) = subdir {
        prefix.extend(path_components(&subdir.to_string_lossy()));
    }
    prefix.extend(path_components(dir));

    let mut found_prefix = false;
    let mut dirs = std::collections::BTreeSet::new();
    for name in archive.file_names() {
        let explicit_dir_entry = name.ends_with('/') || name.ends_with('\\');
        let components = path_components(name);
        if components.len() <= prefix.len() || components[..prefix.len()] != prefix[..] {
            continue;
        }
        found_prefix = true;
        if components.len() > prefix.len() + 1 || explicit_dir_entry {
            dirs.insert(components[prefix.len()].clone());
        }
    }

    if !found_prefix && !prefix.is_empty() {
        return Err(MissingSource::ArchiveEntry { path: prefix.join("/") }.into());
    }
    Ok(dirs.into_iter().collect())
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

/// Whether the archive contains `dir` as a directory component, scoped to the
/// receiver's working subdirectory when one is set. Used for platform
/// indicators such as the `syscalls` folder.
pub(crate) fn archive_has_dir<'a>(
    mut file_names: impl Iterator<Item = &'a str>,
    subdir: Option<&PathBuf>,
    dir: &str,
) -> bool {
    let subdir_components: Vec<String> = subdir
        .map(|subdir| {
            subdir
                .to_string_lossy()
                .replace('\\', "/")
                .split('/')
                .filter(|c| !c.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    file_names.any(|name| {
        let explicit_dir_entry = name.ends_with('/') || name.ends_with('\\');
        let components: Vec<&str> = name.split(['/', '\\']).filter(|c| !c.is_empty()).collect();
        // The directory must appear after the subdir components when scoped.
        // A non-terminal component proves a directory through a child path;
        // a terminal component only counts when the zip has an explicit
        // directory entry such as `syscalls/`.
        let start = if subdir_components.is_empty() {
            0
        } else {
            // `subdir` is a working-directory prefix (see `resolve_archive_path`),
            // so it only counts at the archive root or directly beneath a single
            // top-level bundle directory. Accepting it at any depth would let an
            // unrelated subtree that repeats the same component names answer for
            // this receiver.
            const MAX_BUNDLE_ROOT_DEPTH: usize = 1;
            match components
                .windows(subdir_components.len())
                .position(|window| window.iter().zip(&subdir_components).all(|(a, b)| a == b))
            {
                Some(pos) if pos <= MAX_BUNDLE_ROOT_DEPTH => pos + subdir_components.len(),
                _ => return false,
            }
        };
        components[start..].iter().enumerate().any(|(index, component)| {
            *component == dir && (index + 1 < components[start..].len() || explicit_dir_entry)
        })
    })
}

pub fn resolve_archive_path<A: Read + Seek>(
    subdir: Option<&PathBuf>,
    archive: &mut ZipArchive<A>,
    filename: &str,
) -> Result<String> {
    fn normalize_archive_separators(path: &str) -> String {
        path.replace('\\', "/")
    }

    let mut workdir = PathBuf::from(archive.by_index(0)?.name().to_string());
    trim_to_working_directory(&mut workdir);
    if let Some(dir) = subdir {
        workdir.push(dir);
    }
    let path = normalize_archive_separators(workdir.join(filename).to_string_lossy().as_ref());
    if archive.by_name(&path).is_ok() {
        return Ok(path);
    }

    // ECK writes some entries as `name//file`. Match on the same components
    // `list_archive_dirs` discovers, so every listed directory can be read.
    let wanted = path_components(&path);
    let found = archive
        .file_names()
        .find(|name| !name.ends_with(['/', '\\']) && path_components(name) == wanted)
        .map(str::to_string);
    found.ok_or_else(|| MissingSource::ArchiveEntry { path }.into())
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

            zip.start_file("version.json", SimpleFileOptions::default()).unwrap();
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
    fn every_listed_directory_resolves_through_double_slash_entries() {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            zip.start_file("version.json", SimpleFileOptions::default()).unwrap();
            zip.write_all(b"{}").unwrap();
            zip.start_file("demo//pods.json", SimpleFileOptions::default()).unwrap();
            zip.write_all(b"{}").unwrap();
            zip.finish().unwrap();
        }
        let mut archive = ZipArchive::new(Cursor::new(buf)).unwrap();

        assert_eq!(list_archive_dirs(None, &mut archive, "").unwrap(), vec!["demo"]);
        assert_eq!(
            resolve_archive_path(None, &mut archive, "demo/pods.json").unwrap(),
            "demo//pods.json"
        );
        assert!(resolve_archive_path(None, &mut archive, "demo/events.json").is_err());
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
            zip.start_file("file.json", SimpleFileOptions::default()).unwrap();
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

    #[test]
    fn archive_has_dir_matches_explicit_terminal_directory_entries() {
        assert!(archive_has_dir(["syscalls/"].into_iter(), None, "syscalls"));
        assert!(archive_has_dir(
            ["root/cluster/syscalls/"].into_iter(),
            Some(&PathBuf::from("cluster")),
            "syscalls"
        ));
    }

    #[test]
    fn archive_has_dir_matches_implicit_directory_child_entries() {
        assert!(archive_has_dir(
            ["syscalls/processes.txt"].into_iter(),
            None,
            "syscalls"
        ));
        assert!(archive_has_dir(
            ["root/cluster/syscalls/processes.txt"].into_iter(),
            Some(&PathBuf::from("cluster")),
            "syscalls"
        ));
    }

    #[test]
    fn archive_has_dir_ignores_a_matching_component_in_an_unrelated_subtree() {
        assert!(!archive_has_dir(
            ["root/child-b/child-a/syscalls/processes.txt"].into_iter(),
            Some(&PathBuf::from("child-a")),
            "syscalls"
        ));
    }

    #[test]
    fn archive_has_dir_rejects_terminal_file_entries() {
        assert!(!archive_has_dir(["syscalls"].into_iter(), None, "syscalls"));
        assert!(!archive_has_dir(
            ["root/cluster/syscalls"].into_iter(),
            Some(&PathBuf::from("cluster")),
            "syscalls"
        ));
    }
}
