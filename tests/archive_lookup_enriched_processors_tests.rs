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
    for archive in archive_fixtures() {
        let output_dir = process_archive(&archive);

        let settings_node = first_doc(&output_dir, "settings-node-esdiag.ndjson");
        assert_node_lookup_enrichment(&settings_node["node"], &archive, "settings-node-esdiag");

        let metrics_node = first_doc(&output_dir, "metrics-node-esdiag.ndjson");
        assert_node_lookup_enrichment(&metrics_node["node"], &archive, "metrics-node-esdiag");

        let metrics_task = first_doc(&output_dir, "metrics-task-esdiag.ndjson");
        assert_node_lookup_enrichment(&metrics_task["node"], &archive, "metrics-task-esdiag");

        let metrics_shard = first_doc(&output_dir, "metrics-shard-esdiag.ndjson");
        assert_node_lookup_enrichment(&metrics_shard["node"], &archive, "metrics-shard-esdiag");

        let metrics_http_clients = first_doc(&output_dir, "metrics-node.http.clients-esdiag.ndjson");
        assert_node_lookup_enrichment(
            &metrics_http_clients["node"],
            &archive,
            "metrics-node.http.clients-esdiag",
        );

        let metrics_cluster_applier = first_doc(&output_dir, "metrics-node.discovery.cluster_applier-esdiag.ndjson");
        assert_node_lookup_enrichment(
            &metrics_cluster_applier["node"],
            &archive,
            "metrics-node.discovery.cluster_applier-esdiag",
        );
    }
}

#[test]
fn test_archive_node_lookup_specific_os_values_for_9_1_fixture() {
    let archive = Path::new("tests/archives/elasticsearch-api-diagnostics-9.1.3.zip");
    assert!(archive.exists(), "missing archive fixture: {}", archive.display());

    let output_dir = process_archive(archive);
    let metrics_node = first_doc(&output_dir, "metrics-node-esdiag.ndjson");
    let node = &metrics_node["node"];

    assert_eq!(node["name"], "esdiag-node");
    assert_eq!(node["os"]["allocated_processors"], 8);
    assert_eq!(node["os"]["available_processors"], 8);
    assert_eq!(node["os"]["pretty_name"], "Red Hat Enterprise Linux 9.6 (Plow)");
}

#[test]
fn test_archive_data_stream_lookup_enrichment_for_index_processors() {
    for archive in archive_fixtures() {
        let output_dir = process_archive(&archive);

        let settings_index_docs = read_docs(&output_dir, "settings-index-esdiag.ndjson");
        let settings_index_doc = settings_index_docs
            .iter()
            .find(|doc| doc["index"]["data_stream"]["name"].is_string())
            .unwrap_or_else(|| {
                panic!(
                    "settings-index-esdiag in {} had no document with data_stream enrichment",
                    archive.display()
                )
            });
        assert_has_data_stream_lookup(settings_index_doc, &archive, "settings-index-esdiag");

        let metrics_index_docs = read_docs(&output_dir, "metrics-index-esdiag.ndjson");
        let metrics_index_doc = metrics_index_docs
            .iter()
            .find(|doc| doc["index"]["data_stream"]["name"].is_string())
            .unwrap_or_else(|| {
                panic!(
                    "metrics-index-esdiag in {} had no document with data_stream enrichment",
                    archive.display()
                )
            });
        assert_has_data_stream_lookup(metrics_index_doc, &archive, "metrics-index-esdiag");
    }
}

#[test]
fn test_archive_index_settings_lookup_enrichment_for_index_metrics() {
    for archive in archive_fixtures() {
        let output_dir = process_archive(&archive);

        let metrics_index_docs = read_docs(&output_dir, "metrics-index-esdiag.ndjson");
        let metrics_index_doc = metrics_index_docs
            .iter()
            .find(|doc| {
                doc["index"]["number_of_shards"].is_number()
                    && doc["index"]["number_of_replicas"].is_number()
                    && doc["index"]["refresh_interval"].is_string()
            })
            .unwrap_or_else(|| {
                panic!(
                    "metrics-index-esdiag in {} had no document with index settings enrichment",
                    archive.display()
                )
            });
        assert_has_index_settings_lookup(metrics_index_doc, &archive, "metrics-index-esdiag");

        let metrics_shard_docs = read_docs(&output_dir, "metrics-shard-esdiag.ndjson");
        let metrics_shard_doc = metrics_shard_docs
            .iter()
            .find(|doc| {
                doc["index"]["number_of_shards"].is_number()
                    && doc["index"]["number_of_replicas"].is_number()
                    && doc["index"]["refresh_interval"].is_string()
            })
            .unwrap_or_else(|| {
                panic!(
                    "metrics-shard-esdiag in {} had no document with index settings enrichment",
                    archive.display()
                )
            });
        assert_has_index_settings_lookup(metrics_shard_doc, &archive, "metrics-shard-esdiag");
    }
}
