// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::processor::{
    DataSource, SourceContext, StreamingDataSource,
    diagnostic::{DiagnosticManifest, Manifest},
};
use crate::receiver::{MissingSource, SourceInstance, is_source_instance_path, source_has_json};
use eyre::{Result, WrapErr, eyre};
use futures::stream::{self, BoxStream};
use serde::de::DeserializeOwned;
use std::{
    collections::HashMap,
    io::{BufReader, Read, Seek},
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::mpsc;
use zip::ZipArchive;

mod bytes;
mod file;
mod scrub;

pub use bytes::*;
pub use file::*;
pub(crate) use scrub::with_normalized_json_reader;
pub use scrub::{normalize_supported_content, supports_json_normalization};

// Entries at or below this size are read into memory and parsed with `from_slice`.
// Larger non-streaming entries fall back to a 256 KB buffered `from_reader` to
// cap peak RSS on multi-hundred-MB files (e.g., settings.json).
pub(crate) const MAX_SLICE_READ_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const STREAM_BUF_CAPACITY: usize = 256 * 1024;

pub(crate) fn capped_prealloc_capacity(zip_size: u64) -> usize {
    zip_size.min(MAX_SLICE_READ_BYTES) as usize
}

pub(crate) fn slice_has_json(bytes: &[u8]) -> bool {
    bytes.iter().any(|b| !matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
}

pub fn get_all_from_archive<R, T>(
    archive: &mut IndexedArchive<R>,
    subdir: Option<&PathBuf>,
    ctx: &SourceContext,
    scrubbed: bool,
) -> Result<Vec<SourceInstance<T>>>
where
    R: Read + Seek,
    T: DataSource + DeserializeOwned,
{
    let candidates = T::candidate_source_file_paths(ctx)?;
    let mut root = archive_root(archive)?;
    if let Some(subdir) = subdir {
        root.push(subdir);
    }
    let root = root.to_string_lossy().replace('\\', "/");
    let root = root.trim_matches('/');

    // `(relative path, entry name)`: entries are read by their stored name,
    // which may use `\` separators.
    let mut filenames: Vec<(String, String)> = archive
        .file_names()
        .filter_map(|filename| {
            let normalized = filename.replace('\\', "/");
            let relative = strip_archive_root(&normalized, root)?.to_string();
            is_source_instance_path(&relative, &candidates).then(|| (relative, filename.to_string()))
        })
        .collect();
    filenames.sort();
    filenames.dedup_by(|a, b| a.0 == b.0);

    if filenames.is_empty() {
        return Err(MissingSource::NoCandidates { source: T::name() }.into());
    }

    Ok(filenames
        .into_iter()
        .map(|(path, filename)| SourceInstance {
            data: read_archive_source(&mut archive.zip, filename, scrubbed),
            path,
        })
        .collect())
}

fn read_archive_source<R, T>(archive: &mut ZipArchive<R>, filename: String, scrubbed: bool) -> Result<T>
where
    R: Read + Seek,
    T: DataSource + DeserializeOwned,
{
    let mut file = archive
        .by_name(&filename)
        .wrap_err_with(|| format!("Failed to read {filename} from archive"))?;
    if (scrubbed && supports_json_normalization(&filename)) || file.size() > MAX_SLICE_READ_BYTES {
        let mut reader = BufReader::with_capacity(STREAM_BUF_CAPACITY, file);
        if !source_has_json(&mut reader).wrap_err_with(|| format!("Failed to read {filename} for {}", T::name()))? {
            return Err(MissingSource::Empty { path: filename }.into());
        }
        return with_normalized_json_reader(&filename, reader, scrubbed, |reader| {
            serde_json::from_reader(reader).map_err(Into::into)
        })
        .wrap_err_with(|| format!("Failed to parse {filename} for {}", T::name()));
    }
    let mut buf = Vec::with_capacity(capped_prealloc_capacity(file.size()));
    file.read_to_end(&mut buf)
        .wrap_err_with(|| format!("Failed to read {filename} for {}", T::name()))?;
    if !slice_has_json(&buf) {
        return Err(MissingSource::Empty { path: filename }.into());
    }
    serde_json::from_slice(&buf).wrap_err_with(|| format!("Failed to parse {filename} for {}", T::name()))
}

fn strip_archive_root<'a>(filename: &'a str, root: &str) -> Option<&'a str> {
    if root.is_empty() {
        return Some(filename);
    }
    let relative = filename.strip_prefix(root)?.strip_prefix('/')?;
    Some(relative.trim_start_matches('/'))
}

/// Immutable archive-wide metadata computed once when a receiver opens an archive:
/// irregular entry-name mappings (`name//file` -> stored name) and the resolved
/// bundle root directory. Shared across pooled handles via `Arc` so opening or
/// cloning handles never rescans the archive's entry table.
#[derive(Clone, Debug)]
pub(crate) struct ArchiveIndex {
    irregular: HashMap<String, String>,
    root: Option<PathBuf>,
}

impl ArchiveIndex {
    pub(crate) fn from_zip<R: Read + Seek>(zip: &mut ZipArchive<R>) -> Self {
        let irregular = zip
            .file_names()
            .filter(|name| !name.ends_with(['/', '\\']))
            .filter_map(|name| {
                let normalized = path_components(name).join("/");
                (normalized != name).then(|| (normalized, name.to_string()))
            })
            .collect();
        let root = compute_archive_root(zip).ok();
        Self { irregular, root }
    }
}

/// A ZIP archive paired with shared, precomputed archive metadata (`ArchiveIndex`).
pub struct IndexedArchive<R> {
    zip: ZipArchive<R>,
    index: Arc<ArchiveIndex>,
}

impl<R: Read + Seek> IndexedArchive<R> {
    pub fn new(mut zip: ZipArchive<R>) -> Self {
        let index = Arc::new(ArchiveIndex::from_zip(&mut zip));
        Self { zip, index }
    }

    pub(crate) fn with_index(zip: ZipArchive<R>, index: Arc<ArchiveIndex>) -> Self {
        Self { zip, index }
    }

    pub(crate) fn index(&self) -> &Arc<ArchiveIndex> {
        &self.index
    }
}

impl<R> Deref for IndexedArchive<R> {
    type Target = ZipArchive<R>;

    fn deref(&self) -> &Self::Target {
        &self.zip
    }
}

impl<R> DerefMut for IndexedArchive<R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.zip
    }
}

/// Thread-safe pool of reusable `IndexedArchive` handles.
///
/// Concurrent readers each check out their own independent handle (no read-time
/// lock contention). When a read completes, the handle is returned to the pool
/// so subsequent reads reuse its already-parsed ZIP central directory without
/// re-opening the file or re-parsing ZIP metadata.
pub(crate) struct ArchivePool<R> {
    handles: std::sync::Mutex<Vec<IndexedArchive<R>>>,
    index: Arc<ArchiveIndex>,
}

impl<R: Read + Seek> ArchivePool<R> {
    pub(crate) fn new(initial: IndexedArchive<R>) -> Self {
        let index = Arc::clone(initial.index());
        Self {
            handles: std::sync::Mutex::new(vec![initial]),
            index,
        }
    }

    pub(crate) fn checkout<F>(pool: &Arc<Self>, open_zip: F) -> Result<PooledArchive<R>>
    where
        F: FnOnce() -> Result<ZipArchive<R>>,
    {
        let handle = {
            let mut guard = pool.handles.lock().unwrap_or_else(|e| e.into_inner());
            guard.pop()
        };
        let archive = match handle {
            Some(existing) => existing,
            None => IndexedArchive::with_index(open_zip()?, Arc::clone(&pool.index)),
        };
        Ok(PooledArchive {
            archive: Some(archive),
            pool: Some(Arc::clone(pool)),
        })
    }

    fn checkin(&self, archive: IndexedArchive<R>) {
        const MAX_POOLED_HANDLES: usize = 16;
        let mut guard = self.handles.lock().unwrap_or_else(|e| e.into_inner());
        if guard.len() < MAX_POOLED_HANDLES {
            guard.push(archive);
        }
    }
}

/// RAII guard that returns an `IndexedArchive` handle to its `ArchivePool` on drop.
pub(crate) struct PooledArchive<R: Read + Seek> {
    archive: Option<IndexedArchive<R>>,
    pool: Option<Arc<ArchivePool<R>>>,
}

impl<R: Read + Seek> Deref for PooledArchive<R> {
    type Target = IndexedArchive<R>;

    fn deref(&self) -> &Self::Target {
        self.archive.as_ref().expect("PooledArchive checked out")
    }
}

impl<R: Read + Seek> DerefMut for PooledArchive<R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.archive.as_mut().expect("PooledArchive checked out")
    }
}

impl<R: Read + Seek> Drop for PooledArchive<R> {
    fn drop(&mut self) {
        if let (Some(pool), Some(archive)) = (self.pool.take(), self.archive.take()) {
            pool.checkin(archive);
        }
    }
}
#[derive(Clone, Copy)]
enum ArchiveOpeningError {
    Zip,
    MemoryContext,
}

fn get_from_archive<R, T>(
    archive: &mut IndexedArchive<R>,
    subdir: Option<&PathBuf>,
    ctx: &SourceContext,
    scrubbed: bool,
    opening_error: ArchiveOpeningError,
) -> Result<T>
where
    R: Read + Seek,
    T: DeserializeOwned + DataSource,
{
    let source_paths = T::candidate_source_file_paths(ctx)?;
    let mut last_resolve_error = None;

    for source_path in source_paths {
        let filename = match resolve_archive_path(subdir, archive, &source_path) {
            Ok(filename) => filename,
            Err(error) => {
                last_resolve_error = Some(error);
                continue;
            }
        };
        if scrubbed {
            tracing::debug!("Reading {} (scrubbed mode)", filename);
        } else {
            tracing::debug!("Reading {}", filename);
        }
        let mut file = archive.by_name(&filename).map_err(|error| match opening_error {
            ArchiveOpeningError::Zip => eyre::Report::from(error),
            ArchiveOpeningError::MemoryContext => eyre!("Failed to read file {filename} from archive: {error}"),
        })?;
        if (scrubbed && supports_json_normalization(&filename)) || file.size() > MAX_SLICE_READ_BYTES {
            let mut reader = BufReader::with_capacity(STREAM_BUF_CAPACITY, file);
            if !source_has_json(&mut reader)? {
                last_resolve_error = Some(MissingSource::Empty { path: filename }.into());
                continue;
            }
            return with_normalized_json_reader(&filename, reader, scrubbed, |reader| {
                Ok(serde_json::from_reader(reader)?)
            })
            .wrap_err_with(|| format!("Failed to parse {filename} for {}", T::name()));
        }
        let mut buf = Vec::with_capacity(capped_prealloc_capacity(file.size()));
        file.read_to_end(&mut buf)?;
        if !slice_has_json(&buf) {
            last_resolve_error = Some(MissingSource::Empty { path: filename }.into());
            continue;
        }
        return serde_json::from_slice(&buf).wrap_err_with(|| format!("Failed to parse {filename} for {}", T::name()));
    }

    match last_resolve_error {
        Some(error) => Err(error),
        None => Err(MissingSource::NoCandidates { source: T::name() }.into()),
    }
}

pub async fn get_stream_from_archive<R, T>(
    mut archive: PooledArchive<R>,
    subdir: Option<PathBuf>,
    ctx: SourceContext,
    scrubbed: bool,
) -> Result<BoxStream<'static, Result<T::Item>>>
where
    R: Read + Seek + Send + Sync + 'static,
    T: StreamingDataSource + DeserializeOwned + DataSource,
    T::Item: DeserializeOwned + Send + 'static,
{
    let (tx, rx) = mpsc::channel(100);

    let tx_err = tx.clone();
    let handle = tokio::task::spawn_blocking(move || {
        let source_path = match T::resolve_source_file_path(&ctx) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.blocking_send(Err(eyre::eyre!(e)));
                return;
            }
        };
        let filename = match resolve_archive_path(subdir.as_ref(), &mut archive, &source_path) {
            Ok(f) => f,
            Err(e) => {
                let _ = tx.blocking_send(Err(eyre::eyre!(e)));
                return;
            }
        };

        tracing::debug!("Streaming from archive: {}", filename);
        let stream_result = match archive.by_name(&filename) {
            Ok(file) => {
                let reader = BufReader::with_capacity(STREAM_BUF_CAPACITY, file);
                with_normalized_json_reader(&filename, reader, scrubbed, |reader| {
                    T::deserialize_stream_buffered(reader, tx.clone())
                })
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

/// A bundle root is the archive root or a single top-level directory.
const MAX_BUNDLE_ROOT_DEPTH: usize = 1;

/// The bundle root inside an archive.
///
/// Collectors write entries concurrently, so the first entry can be a scoped
/// file such as `spaces/default/kibana_alerts.json`. Prefer the directory of
/// a manifest within `MAX_BUNDLE_ROOT_DEPTH`; deeper manifests belong to
/// included diagnostics.
fn compute_archive_root<A: Read + Seek>(archive: &mut ZipArchive<A>) -> Result<PathBuf> {
    let manifest_root = archive
        .file_names()
        .filter_map(|name| {
            let components: Vec<&str> = name.split(['/', '\\']).filter(|c| !c.is_empty()).collect();
            let (file, parents) = components.split_last()?;
            let is_manifest = *file == Manifest::FILENAME || *file == DiagnosticManifest::FILENAME;
            (is_manifest && parents.len() <= MAX_BUNDLE_ROOT_DEPTH).then(|| (parents.len(), parents.join("/")))
        })
        .min();
    if let Some((_, root)) = manifest_root {
        return Ok(PathBuf::from(root));
    }
    let mut root = PathBuf::from(archive.by_index(0)?.name());
    trim_to_working_directory(&mut root);
    Ok(root)
}

fn archive_root<A: Read + Seek>(archive: &mut IndexedArchive<A>) -> Result<PathBuf> {
    if let Some(root) = &archive.index.root {
        return Ok(root.clone());
    }
    compute_archive_root(&mut archive.zip)
}

/// Stream the lines of one archive entry using an exclusive pooled handle.
pub async fn read_lines_from_archive<R>(
    mut archive: PooledArchive<R>,
    subdir: Option<PathBuf>,
    filename: &str,
) -> Result<BoxStream<'static, Result<String>>>
where
    R: Read + Seek + Send + Sync + 'static,
{
    let path = resolve_archive_path(subdir.as_ref(), &mut archive, filename)?;
    tracing::debug!("Streaming lines from archive: {}", path);
    let (tx, rx) = mpsc::channel(256);
    let producer_tx = tx.clone();
    let handle = tokio::task::spawn_blocking(move || match archive.by_name(&path) {
        Ok(file) => crate::receiver::send_bundle_lines(BufReader::new(file), &producer_tx),
        Err(err) => {
            let _ = producer_tx.blocking_send(Err(eyre::eyre!(err)));
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
    archive: &mut IndexedArchive<A>,
    dir: &str,
) -> Result<Vec<String>> {
    let root = archive_root(archive)?;
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
    archive: &mut IndexedArchive<A>,
    filename: &str,
) -> Result<String> {
    fn normalize_archive_separators(path: &str) -> String {
        path.replace('\\', "/")
    }

    let mut workdir = archive_root(archive)?;
    if let Some(dir) = subdir {
        workdir.push(dir);
    }
    let path = normalize_archive_separators(workdir.join(filename).to_string_lossy().as_ref());
    let normalized = path_components(&path).join("/");
    if archive.index_for_name(&normalized).is_some() {
        return Ok(normalized);
    }
    match archive.index.irregular.get(&normalized) {
        Some(name) => Ok(name.clone()),
        None => Err(MissingSource::ArchiveEntry { path }.into()),
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
        let mut archive = IndexedArchive::new(ZipArchive::new(Cursor::new(archive_data)).unwrap());

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
            zip.start_file("ops//pod//operator-0//logs.txt", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"line").unwrap();
            zip.finish().unwrap();
        }
        let mut archive = IndexedArchive::new(ZipArchive::new(Cursor::new(buf)).unwrap());

        assert_eq!(list_archive_dirs(None, &mut archive, "").unwrap(), vec!["demo", "ops"]);
        assert_eq!(
            list_archive_dirs(None, &mut archive, "ops/pod").unwrap(),
            vec!["operator-0"]
        );
        assert_eq!(
            resolve_archive_path(None, &mut archive, "ops/pod/operator-0/logs.txt").unwrap(),
            "ops//pod//operator-0//logs.txt"
        );
        assert_eq!(
            resolve_archive_path(None, &mut archive, "demo/pods.json").unwrap(),
            "demo//pods.json"
        );
        assert!(resolve_archive_path(None, &mut archive, "demo/events.json").is_err());
    }

    #[test]
    fn resolve_archive_path_with_single_slash_returns_ok() {
        let archive_data = create_test_archive_with_single_slash();
        let mut archive = IndexedArchive::new(ZipArchive::new(Cursor::new(archive_data)).unwrap());

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
        let mut archive = IndexedArchive::new(ZipArchive::new(Cursor::new(archive_data)).unwrap());

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

        let mut archive = IndexedArchive::new(ZipArchive::new(Cursor::new(buf)).unwrap());
        let result = resolve_archive_path(None, &mut archive, "file.json").unwrap();

        // Should derive path from first entry in archive
        assert_eq!(result, "file.json");
    }

    #[test]
    fn archive_root_ignores_scoped_first_entries() {
        for prefix in ["", "bundle/"] {
            let mut buf = Vec::new();
            {
                let mut zip = ZipWriter::new(Cursor::new(&mut buf));
                for name in [
                    "spaces/default/kibana_alerts.json",
                    "pages/page-0001/kibana_fleet_agents.json",
                    "diagnostic_manifest.json",
                    "kibana_status.json",
                ] {
                    zip.start_file(format!("{prefix}{name}"), SimpleFileOptions::default())
                        .unwrap();
                    zip.write_all(b"{}").unwrap();
                }
                zip.finish().unwrap();
            }

            let mut archive = IndexedArchive::new(ZipArchive::new(Cursor::new(buf)).unwrap());
            let result = resolve_archive_path(None, &mut archive, "kibana_status.json").unwrap();
            assert_eq!(result, format!("{prefix}kibana_status.json"));
        }
    }

    #[test]
    fn archive_root_ignores_included_diagnostic_manifests() {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            for name in [
                "eck/namespace/kibana/kb/diagnostic_manifest.json",
                "eck/manifest.json",
                "eck/namespace/kibana/kb/kibana_status.json",
            ] {
                zip.start_file(name, SimpleFileOptions::default()).unwrap();
                zip.write_all(b"{}").unwrap();
            }
            zip.finish().unwrap();
        }

        let mut archive = IndexedArchive::new(ZipArchive::new(Cursor::new(buf)).unwrap());
        let subdir = PathBuf::from("namespace/kibana/kb");
        let result = resolve_archive_path(Some(&subdir), &mut archive, "kibana_status.json").unwrap();
        assert_eq!(result, "eck/namespace/kibana/kb/kibana_status.json");
    }

    #[test]
    fn missing_file_returns_err() {
        let archive_data = create_test_archive_with_single_slash();
        let mut archive = IndexedArchive::new(ZipArchive::new(Cursor::new(archive_data)).unwrap());
        let result = resolve_archive_path(None, &mut archive, "missing.json");
        assert!(result.unwrap_err().to_string().contains("File not found"));
    }

    #[test]
    fn archive_root_requires_a_component_boundary() {
        assert_eq!(
            strip_archive_root("bundle/spaces/default/kibana_alerts.json", "bundle"),
            Some("spaces/default/kibana_alerts.json")
        );
        assert_eq!(
            strip_archive_root("bundle//spaces/default/kibana_alerts.json", "bundle"),
            Some("spaces/default/kibana_alerts.json")
        );
        assert_eq!(
            strip_archive_root("bundlespaces/default/kibana_alerts.json", "bundle"),
            None
        );
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

    #[test]
    fn capped_prealloc_capacity_caps_before_casting_to_usize() {
        assert_eq!(capped_prealloc_capacity(42), 42);
        assert_eq!(capped_prealloc_capacity(u64::MAX), MAX_SLICE_READ_BYTES as usize);
    }

    #[test]
    fn archive_pool_reuses_handles_and_shares_index() {
        let archive_data = create_test_archive_with_double_slash();
        let initial = IndexedArchive::new(ZipArchive::new(Cursor::new(archive_data.clone())).unwrap());
        let pool = Arc::new(ArchivePool::new(initial));
        let mut open_calls = 0usize;

        // Sequential checkouts reuse the initial pooled handle without calling open_zip.
        {
            let mut first = ArchivePool::checkout(&pool, || {
                open_calls += 1;
                Ok(ZipArchive::new(Cursor::new(archive_data.clone()))?)
            })
            .unwrap();
            assert_eq!(open_calls, 0);
            assert_eq!(
                resolve_archive_path(None, &mut first, "namespace/elasticsearch/cluster-one/version.json").unwrap(),
                "namespace/elasticsearch/cluster-one//version.json"
            );
        }
        {
            let _second = ArchivePool::checkout(&pool, || {
                open_calls += 1;
                Ok(ZipArchive::new(Cursor::new(archive_data.clone()))?)
            })
            .unwrap();
            assert_eq!(open_calls, 0);

            // Concurrent checkout opens one additional handle sharing the same Arc<ArchiveIndex>.
            let concurrent = ArchivePool::checkout(&pool, || {
                open_calls += 1;
                Ok(ZipArchive::new(Cursor::new(archive_data.clone()))?)
            })
            .unwrap();
            assert_eq!(open_calls, 1);
            assert!(Arc::ptr_eq(_second.index(), concurrent.index()));
        }
    }
}
