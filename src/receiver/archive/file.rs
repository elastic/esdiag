// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{normalize_supported_content, normalize_supported_reader_to_temp, resolve_archive_path, supports_json_normalization};
use crate::{
    processor::{DataSource, SourceContext, StreamingDataSource},
    receiver::{RawResponse, Receive, ReceiveMultiple, ReceiveRaw},
};
use eyre::{Result, eyre};
use futures::stream::BoxStream;
use serde::de::DeserializeOwned;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
    sync::Arc,
    sync::OnceLock,
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
    source_product: Arc<OnceLock<&'static str>>,
    scrubbed: bool,
}

impl TryFrom<PathBuf> for ArchiveFileReceiver {
    type Error = eyre::Report;

    fn try_from(path: PathBuf) -> Result<Self> {
        let filename = format!("{}", path.file_name().unwrap_or_default().display());
        match path.is_file() {
            true => {
                tracing::debug!("File is valid: {}", path.display());
                let file = File::open(path)?;
                let modified_date = file.metadata()?.modified()?;
                let archive = ZipArchive::new(file)?;
                Ok(Self {
                    archive: Arc::new(RwLock::new(archive)),
                    modified_date,
                    filename,
                    subdir: None,
                    source_product: Arc::new(OnceLock::new()),
                    scrubbed: false,
                })
            }
            false => {
                tracing::debug!("File is invalid: {}", path.display());
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
        if tracing::enabled!(tracing::Level::TRACE) {
            let file_names: Vec<String> = archive.file_names().map(|name| name.to_string()).collect();
            tracing::trace!("Files in archive: {:?}", file_names);
        }
        tracing::debug!("Archive {} is valid: {}", &self.filename, !is_empty);
        !is_empty
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
        let ctx = self.source_context()?;
        let source_paths = T::candidate_source_file_paths(&ctx)?;
        let mut last_resolve_error = None;

        for source_path in source_paths {
            match resolve_archive_path(self.subdir.as_ref(), &mut *archive, &source_path) {
                Ok(filename) => {
                    if self.scrubbed {
                        tracing::debug!("Reading {} (scrubbed mode)", filename);
                    } else {
                        tracing::debug!("Reading {}", filename);
                    }
                    let file = archive.by_name(&filename)?;
                    let data: T = if self.scrubbed && supports_json_normalization(&filename) {
                        let reader = BufReader::new(file);
                        let mut transformed = normalize_supported_reader_to_temp(&filename, reader)?;
                        tracing::debug!(
                            "Unscrubbed {} address fields in {}",
                            transformed.transformed_fields,
                            filename
                        );
                        let reader = BufReader::new(transformed.file.as_file_mut());
                        serde_json::from_reader(reader)?
                    } else {
                        if self.scrubbed {
                            tracing::debug!("Scrubbed mode read {} (no normalization rules)", filename);
                        }
                        let reader = BufReader::new(file);
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
            None => Err(eyre!("No candidate source files available for {}", T::name())),
        }
    }

    async fn get_stream<T>(&self) -> Result<BoxStream<'static, Result<T::Item>>>
    where
        T: StreamingDataSource + DeserializeOwned,
        T::Item: DeserializeOwned + Send + 'static,
    {
        let ctx = self.source_context()?;
        super::get_stream_from_archive::<File, T>(self.archive.clone(), self.subdir.clone(), ctx, self.scrubbed).await
    }
}

impl ReceiveRaw for ArchiveFileReceiver {
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
        let mut archive = self.archive.write().await;
        let ctx = self.source_context()?;
        let source_paths = T::candidate_source_file_paths(&ctx)?;
        let mut last_resolve_error = None;

        for source_path in source_paths {
            match resolve_archive_path(self.subdir.as_ref(), &mut *archive, &source_path) {
                Ok(filename) => {
                    if self.scrubbed {
                        tracing::debug!("Reading {} (scrubbed mode)", filename);
                    } else {
                        tracing::debug!("Reading {}", filename);
                    }
                    let file = archive.by_name(&filename)?;
                    let mut reader = BufReader::new(file);
                    let mut data = String::new();
                    reader.read_to_string(&mut data)?;
                    let body = if self.scrubbed {
                        let transformed = normalize_supported_content(&filename, data)?;
                        if transformed.supported {
                            tracing::debug!(
                                "Unscrubbed {} address fields in {}",
                                transformed.transformed_fields,
                                filename
                            );
                        } else {
                            tracing::debug!("Scrubbed mode read {} (no normalization rules)", filename);
                        }
                        transformed.content
                    } else {
                        data
                    };
                    let response_size_bytes = body.len() as u64;
                    return Ok(RawResponse {
                        body,
                        status: None,
                        response_time_ms: 0,
                        response_size_bytes,
                    });
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
        tracing::trace!("Setting subdir: {}", work_dir);
        self.subdir = Some(PathBuf::from(work_dir));
        Ok(())
    }
}

impl std::fmt::Display for ArchiveFileReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.filename)
    }
}

impl ArchiveFileReceiver {
    pub(crate) fn clone_for_subdir(&self, work_dir: &str) -> Self {
        Self {
            archive: self.archive.clone(),
            filename: self.filename.clone(),
            subdir: Some(PathBuf::from(work_dir)),
            modified_date: self.modified_date,
            source_product: Arc::new(OnceLock::new()),
        }
    }

    pub async fn read_bundle_json<T>(&self, filename: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let mut archive = self.archive.write().await;
        let filename = resolve_archive_path(self.subdir.as_ref(), &mut *archive, filename)?;
        tracing::debug!("Reading bundle file {}", filename);
        let file = archive.by_name(&filename)?;
        let reader = BufReader::new(file);
        serde_json::from_reader(reader).map_err(Into::into)
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

    pub fn set_scrubbed(&mut self, scrubbed: bool) {
        self.scrubbed = scrubbed;
    }
}

#[cfg(test)]
mod tests {
    use super::super::scrub::synthetic_vectors as v;
    use super::ArchiveFileReceiver;
    use crate::processor::DataSource;
    use crate::receiver::{ReceiveRaw, ScrubMode, should_enable_scrubbed};
    use std::io::Write;
    use std::path::PathBuf;
    use zip::{ZipWriter, write::SimpleFileOptions};

    struct NodesSource;

    impl DataSource for NodesSource {
        fn name() -> String {
            "nodes".to_string()
        }
    }

    fn write_test_archive(base_dir: &str, nodes_json: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let zip_path = dir.path().join("scrubbed-api-diagnostics-test.zip");
        let file = std::fs::File::create(&zip_path).expect("create zip");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        let prefix = format!("{base_dir}/");
        zip.start_file(format!("{prefix}diagnostic_manifest.json"), options)
            .expect("manifest");
        zip.write_all(
            br#"{
  "mode": "full",
  "product": "elasticsearch",
  "type": "elasticsearch_diagnostic",
  "runner": "cli",
  "version": "9.1.3",
  "timestamp": "2025-09-18T00:18:07.432Z"
}"#,
        )
        .expect("manifest body");

        zip.start_file(format!("{prefix}version.json"), options)
            .expect("version");
        zip.write_all(
            br#"{
  "name": "esdiag-node",
  "cluster_name": "esdiag-cluster",
  "cluster_uuid": "aukedefkRcGa0BT16uuuNQ",
  "version": {
    "number": "9.1.3",
    "build_flavor": "default",
    "build_type": "docker",
    "build_hash": "abc",
    "build_date": "2025-01-01T00:00:00.000000000Z",
    "build_snapshot": false,
    "lucene_version": "10.2.2",
    "minimum_wire_compatibility_version": "8.19.0",
    "minimum_index_compatibility_version": "8.0.0"
  },
  "tagline": "You Know, for Search"
}"#,
        )
        .expect("version body");

        zip.start_file(format!("{prefix}nodes.json"), options).expect("nodes");
        zip.write_all(nodes_json.as_bytes()).expect("nodes body");

        zip.finish().expect("finish zip");
        (dir, zip_path)
    }

    #[tokio::test]
    async fn non_scrubbed_archive_passes_nodes_json_unchanged() {
        let archive_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/archives/elasticsearch-api-diagnostics-9.3.3.zip");
        if !archive_path.exists() {
            return;
        }

        let file = std::fs::File::open(&archive_path).expect("open archive");
        let mut archive = zip::ZipArchive::new(file).expect("read archive");
        let entry_path = "nodes.json";
        let mut expected = String::new();
        let mut zip_entry = archive.by_name(entry_path).expect("nodes.json");
        std::io::Read::read_to_string(&mut zip_entry, &mut expected).expect("read nodes.json");

        let mut receiver = ArchiveFileReceiver::try_from(archive_path).expect("receiver");
        receiver.set_scrubbed(false);
        receiver.set_source_product("elasticsearch").expect("source product");

        assert!(!should_enable_scrubbed(
            ScrubMode::Auto,
            Some("elasticsearch-api-diagnostics-9.3.3.zip")
        ));

        let actual = receiver.get_raw::<NodesSource>().await.expect("get raw nodes");
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn scrubbed_receiver_normalizes_malformed_nodes_json() {
        let nodes_json = format!(
            r#"{{
  "_nodes": {{"total": 1, "successful": 1, "failed": 0}},
  "nodes": {{
    "{node_id}": {{
      "name": "{node_id}",
      "transport_address": "{malformed_port}",
      "host": "{malformed}",
      "ip": "{malformed}",
      "version": "9.1.3",
      "build_flavor": "default",
      "build_hash": "abc",
      "build_type": "docker",
      "roles": ["data_hot", "master"],
      "os": {{
        "refresh_interval_in_millis": 1000,
        "available_processors": 8,
        "allocated_processors": 8
      }},
      "jvm": {{}},
      "process": {{}},
      "thread_pool": {{}}
    }}
  }}
}}"#,
            node_id = v::SYNTHETIC_HEX_NODE_ID,
            malformed = v::MALFORMED_IP,
            malformed_port = v::MALFORMED_IP_WITH_PORT,
        );
        let (_dir, zip_path) = write_test_archive("api-diagnostics-scrubbed-test", &nodes_json);
        let mut receiver = ArchiveFileReceiver::try_from(zip_path).expect("receiver");
        receiver.set_scrubbed(true);
        receiver.set_source_product("elasticsearch").expect("source product");

        let raw = receiver.get_raw::<NodesSource>().await.expect("get raw nodes");
        assert!(raw.contains(&format!("\"ip\":\"{}\"", v::NORMALIZED_IP)));
        assert!(raw.contains(&format!("\"host\":\"{}\"", v::NORMALIZED_IP)));
        assert!(raw.contains(&format!("\"transport_address\":\"{}\"", v::NORMALIZED_IP_WITH_PORT)));
    }
}
