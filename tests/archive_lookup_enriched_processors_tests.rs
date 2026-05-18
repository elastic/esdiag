use serde_json::Value;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{TempDir, tempdir};

fn archive_fixtures() -> Vec<PathBuf> {
    let archives_dir = Path::new("tests/archives");
    let mut archives = Vec::new();
    for entry in fs::read_dir(archives_dir).expect("read tests/archives") {
        let entry = entry.expect("archive dir entry");
        let path = entry.path();
        let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
        if filename.starts_with("elasticsearch-api-diagnostics-") && filename.ends_with(".zip") {
            archives.push(path);
        }
    }
    archives.sort();
    assert!(!archives.is_empty(), "no elasticsearch archive fixtures found");
    archives
}

struct ProcessedArchive {
    output: TempDir,
}

impl Deref for ProcessedArchive {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.output.path()
    }
}

fn process_archive(archive: &Path) -> ProcessedArchive {
    let home = tempdir().expect("temp HOME");
    let output = tempdir().expect("temp output");

    let process = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args([
            "process",
            archive.to_str().expect("archive path"),
            output.path().to_str().expect("output path"),
        ])
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .output()
        .expect("run esdiag process");

    assert!(
        process.status.success(),
        "process failed for {}\nstdout:\n{}\nstderr:\n{}",
        archive.display(),
        String::from_utf8_lossy(&process.stdout),
        String::from_utf8_lossy(&process.stderr)
    );

    ProcessedArchive { output }
}

fn first_doc(output_dir: &Path, file_name: &str) -> Value {
    let path = output_dir.join(file_name);
    let content = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {} failed: {}", path.display(), e));
    let line = content
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_else(|| panic!("{} had no documents", path.display()));
    serde_json::from_str::<Value>(line)
        .unwrap_or_else(|e| panic!("failed parsing first doc from {}: {}", path.display(), e))
}

fn first_doc_if_present(output_dir: &Path, file_name: &str) -> Option<Value> {
    let path = output_dir.join(file_name);
    path.exists().then(|| first_doc(output_dir, file_name))
}

fn read_docs(output_dir: &Path, file_name: &str) -> Vec<Value> {
    let path = output_dir.join(file_name);
    let content = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {} failed: {}", path.display(), e));
    let docs: Vec<Value> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|e| panic!("failed parsing document from {}: {}", path.display(), e))
        })
        .collect();
    assert!(!docs.is_empty(), "{} had no documents", path.display());
    docs
}

fn read_docs_if_present(output_dir: &Path, file_name: &str) -> Option<Vec<Value>> {
    let path = output_dir.join(file_name);
    path.exists().then(|| read_docs(output_dir, file_name))
}

fn read_required_lookup_docs(output_dir: &Path, file_name: &str, archive: &Path) -> Option<Vec<Value>> {
    let path = output_dir.join(file_name);
    if path.exists() {
        Some(read_docs(output_dir, file_name))
    } else if archive_requires_lookup_enrichment(archive) {
        panic!("{} is missing required stream {}", archive.display(), file_name);
    } else {
        None
    }
}

fn find_required_doc<'a, P>(docs: &'a [Value], archive: &Path, stream: &str, description: &str, predicate: P) -> &'a Value
where
    P: Fn(&Value) -> bool,
{
    docs.iter().find(|doc| predicate(doc)).unwrap_or_else(|| {
        panic!(
            "{} in {} did not contain a document with {}",
            stream,
            archive.display(),
            description
        )
    })
}

fn archive_requires_lookup_enrichment(archive: &Path) -> bool {
    let filename = archive.file_name().and_then(|name| name.to_str()).unwrap_or_default();
    filename.contains("9.3.3")
}

fn assert_node_lookup_enrichment(node: &Value, archive: &Path, stream: &str) {
    let os = &node["os"];
    assert!(
        os["allocated_processors"].is_number(),
        "{} in {} is missing node.os.allocated_processors",
        stream,
        archive.display()
    );
    assert!(
        os["available_processors"].is_number(),
        "{} in {} is missing node.os.available_processors",
        stream,
        archive.display()
    );
    assert!(
        node["id"].is_string(),
        "{} in {} is missing node.id",
        stream,
        archive.display()
    );
    assert!(
        node["version"].is_string(),
        "{} in {} is missing node.version",
        stream,
        archive.display()
    );
}

fn assert_has_data_stream_lookup(doc: &Value, archive: &Path, stream: &str) {
    assert!(
        doc["index"]["data_stream"]["name"].is_string(),
        "{} in {} missing index.data_stream.name",
        stream,
        archive.display()
    );
    assert!(
        doc["index"]["data_stream"]["dataset"].is_string(),
        "{} in {} missing index.data_stream.dataset",
        stream,
        archive.display()
    );
    assert!(
        doc["index"]["data_stream"]["type"].is_string(),
        "{} in {} missing index.data_stream.type",
        stream,
        archive.display()
    );
    assert!(
        doc["index"]["data_stream"]["namespace"].is_string(),
        "{} in {} missing index.data_stream.namespace",
        stream,
        archive.display()
    );
}

fn assert_has_index_settings_lookup(doc: &Value, archive: &Path, stream: &str) {
    assert!(
        doc["index"]["name"].is_string(),
        "{} in {} missing index.name",
        stream,
        archive.display()
    );
    assert!(
        doc["index"]["number_of_shards"].is_number(),
        "{} in {} missing index.number_of_shards",
        stream,
        archive.display()
    );
    assert!(
        doc["index"]["number_of_replicas"].is_number(),
        "{} in {} missing index.number_of_replicas",
        stream,
        archive.display()
    );
    assert!(
        doc["index"]["refresh_interval"].is_string(),
        "{} in {} missing index.refresh_interval",
        stream,
        archive.display()
    );
}

#[test]
fn test_archive_lookup_enriched_processors_match_expected_shape() {
    let mut checked_streams = 0;
    for archive in archive_fixtures() {
        let output_dir = process_archive(&archive);

        let settings_node = first_doc(&output_dir, "settings-node-esdiag.ndjson");
        assert_node_lookup_enrichment(&settings_node["node"], &archive, "settings-node-esdiag");
        checked_streams += 1;

        if let Some(metrics_node) = first_doc_if_present(&output_dir, "metrics-node-esdiag.ndjson") {
            assert_node_lookup_enrichment(&metrics_node["node"], &archive, "metrics-node-esdiag");
            checked_streams += 1;
        }

        let metrics_task = first_doc(&output_dir, "metrics-task-esdiag.ndjson");
        assert_node_lookup_enrichment(&metrics_task["node"], &archive, "metrics-task-esdiag");
        checked_streams += 1;

        if let Some(metrics_shard) = first_doc_if_present(&output_dir, "metrics-shard-esdiag.ndjson") {
            assert_node_lookup_enrichment(&metrics_shard["node"], &archive, "metrics-shard-esdiag");
            checked_streams += 1;
        }

        if let Some(metrics_http_clients) = first_doc_if_present(&output_dir, "metrics-node.http.clients-esdiag.ndjson")
        {
            assert_node_lookup_enrichment(
                &metrics_http_clients["node"],
                &archive,
                "metrics-node.http.clients-esdiag",
            );
            checked_streams += 1;
        }

        if let Some(metrics_cluster_applier) =
            first_doc_if_present(&output_dir, "metrics-node.discovery.cluster_applier-esdiag.ndjson")
        {
            assert_node_lookup_enrichment(
                &metrics_cluster_applier["node"],
                &archive,
                "metrics-node.discovery.cluster_applier-esdiag",
            );
            checked_streams += 1;
        }
    }
    assert!(checked_streams > 0, "no node lookup streams were checked");
}

#[test]
fn test_archive_node_lookup_specific_os_shape_for_9_3_fixture() {
    let archive = Path::new("tests/archives/elasticsearch-api-diagnostics-9.3.3.zip");
    assert!(archive.exists(), "missing archive fixture: {}", archive.display());

    let output_dir = process_archive(archive);
    let metrics_node = first_doc(&output_dir, "metrics-node-esdiag.ndjson");
    let node = &metrics_node["node"];

    assert!(node["name"].is_string());
    assert!(node["os"]["allocated_processors"].is_number());
    assert!(node["os"]["available_processors"].is_number());
    assert!(node["os"]["pretty_name"].is_string());
}

#[test]
fn test_archive_data_stream_lookup_enrichment_for_index_processors() {
    let mut checked_streams = 0;
    for archive in archive_fixtures() {
        let output_dir = process_archive(&archive);

        if let Some(settings_index_docs) =
            read_required_lookup_docs(&output_dir, "settings-index-esdiag.ndjson", &archive)
        {
            if settings_index_docs
                .iter()
                .any(|doc| doc["index"]["data_stream"]["name"].is_string())
                || archive_requires_lookup_enrichment(&archive)
            {
                let settings_index_doc = find_required_doc(
                    &settings_index_docs,
                    &archive,
                    "settings-index-esdiag",
                    "data stream lookup enrichment",
                    |doc| doc["index"]["data_stream"]["name"].is_string(),
                );
                assert_has_data_stream_lookup(settings_index_doc, &archive, "settings-index-esdiag");
                checked_streams += 1;
            }
        }

        if let Some(metrics_index_docs) =
            read_required_lookup_docs(&output_dir, "metrics-index-esdiag.ndjson", &archive)
        {
            if metrics_index_docs
                .iter()
                .any(|doc| doc["index"]["data_stream"]["name"].is_string())
                || archive_requires_lookup_enrichment(&archive)
            {
                let metrics_index_doc = find_required_doc(
                    &metrics_index_docs,
                    &archive,
                    "metrics-index-esdiag",
                    "data stream lookup enrichment",
                    |doc| doc["index"]["data_stream"]["name"].is_string(),
                );
                assert_has_data_stream_lookup(metrics_index_doc, &archive, "metrics-index-esdiag");
                checked_streams += 1;
            }
        }
    }
    assert!(checked_streams > 0, "no data stream lookup streams were checked");
}

#[test]
fn test_archive_index_settings_lookup_enrichment_for_index_metrics() {
    let mut checked_streams = 0;
    for archive in archive_fixtures() {
        let output_dir = process_archive(&archive);

        if let Some(metrics_index_docs) =
            read_required_lookup_docs(&output_dir, "metrics-index-esdiag.ndjson", &archive)
        {
            let has_index_settings_lookup = |doc: &Value| {
                doc["index"]["number_of_shards"].is_number()
                    && doc["index"]["number_of_replicas"].is_number()
                    && doc["index"]["refresh_interval"].is_string()
            };
            if metrics_index_docs.iter().any(has_index_settings_lookup) || archive_requires_lookup_enrichment(&archive) {
                let metrics_index_doc = find_required_doc(
                    &metrics_index_docs,
                    &archive,
                    "metrics-index-esdiag",
                    "index settings lookup enrichment",
                    has_index_settings_lookup,
                );
                assert_has_index_settings_lookup(metrics_index_doc, &archive, "metrics-index-esdiag");
                checked_streams += 1;
            }
        }

        if let Some(metrics_shard_docs) =
            read_required_lookup_docs(&output_dir, "metrics-shard-esdiag.ndjson", &archive)
        {
            let has_index_settings_lookup = |doc: &Value| {
                doc["index"]["number_of_shards"].is_number()
                    && doc["index"]["number_of_replicas"].is_number()
                    && doc["index"]["refresh_interval"].is_string()
            };
            if metrics_shard_docs.iter().any(has_index_settings_lookup) || archive_requires_lookup_enrichment(&archive) {
                let metrics_shard_doc = find_required_doc(
                    &metrics_shard_docs,
                    &archive,
                    "metrics-shard-esdiag",
                    "index settings lookup enrichment",
                    has_index_settings_lookup,
                );
                assert_has_index_settings_lookup(metrics_shard_doc, &archive, "metrics-shard-esdiag");
                checked_streams += 1;
            }
        }
    }
    assert!(checked_streams > 0, "no index settings lookup streams were checked");
}
