mod scrub_normalization_assertions;

use esdiag::{
    data::Uri,
    exporter::Exporter,
    processor::{Identifiers, Processor},
    receiver::Receiver,
};
use scrub_normalization_assertions::{
    ScrubFixtureExpectations, assert_malformed_ips_preserved_in_node_metrics,
    assert_scrubbed_export, ensure_two_nodes_in_nodes_json,
    ensure_two_nodes_in_nodes_stats_json, ensure_two_nodes_in_tasks_json,
    inject_malformed_ips_in_nodes_json, inject_malformed_ips_in_nodes_stats_json,
    inject_malformed_ips_in_tasks_json,
};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
use tempfile::tempdir;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const GOLDEN_ARCHIVE: &str = "tests/archives/elasticsearch-api-diagnostics-9.3.3.zip";
const ARCHIVE_PREFIX: &str = "";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn build_synthetic_scrubbed_archive(out_zip: &Path) -> ScrubFixtureExpectations {
    let golden = manifest_dir().join(GOLDEN_ARCHIVE);
    let entries = [
        "diagnostic_manifest.json",
        "version.json",
        "cluster_settings_defaults.json",
        "cluster_settings.json",
        "nodes.json",
        "nodes_stats.json",
        "tasks.json",
    ];

    let source_file = fs::File::open(&golden).expect("open golden esdiag test archive");
    let mut source_archive = ZipArchive::new(source_file).expect("read golden archive");
    let dest_file = fs::File::create(out_zip).expect("create synthetic scrubbed zip");
    let mut dest_archive = ZipWriter::new(dest_file);
    let options = SimpleFileOptions::default();

    let mut expectations = None;

    for entry in entries {
        let path = archive_entry_path(entry);
        let mut content = String::new();
        source_archive
            .by_name(&path)
            .unwrap_or_else(|_| panic!("missing {path}"))
            .read_to_string(&mut content)
            .expect("read entry");

        content = match entry {
            "nodes.json" => {
                let content = ensure_two_nodes_in_nodes_json(&content);
                let (updated, exp) = inject_malformed_ips_in_nodes_json(&content);
                expectations = Some(exp);
                updated
            }
            "nodes_stats.json" => {
                let content = ensure_two_nodes_in_nodes_stats_json(&content);
                let exp = expectations
                    .as_ref()
                    .expect("nodes.json must be processed before nodes_stats.json");
                inject_malformed_ips_in_nodes_stats_json(&content, exp)
            }
            "tasks.json" => {
                let content = ensure_two_nodes_in_tasks_json(&content);
                let exp = expectations
                    .as_ref()
                    .expect("nodes.json must be processed before tasks.json");
                inject_malformed_ips_in_tasks_json(&content, exp)
            }
            _ => content,
        };

        dest_archive
            .start_file(path, options)
            .expect("start zip entry");
        dest_archive
            .write_all(content.as_bytes())
            .expect("write zip entry");
    }

    dest_archive.finish().expect("finish synthetic scrubbed zip");
    expectations.expect("nodes.json expectations")
}

fn archive_entry_path(entry: &str) -> String {
    if ARCHIVE_PREFIX.is_empty() {
        entry.to_string()
    } else {
        format!("{ARCHIVE_PREFIX}/{entry}")
    }
}

fn archive_root_path(path: &Path) -> PathBuf {
    if ARCHIVE_PREFIX.is_empty() {
        path.to_path_buf()
    } else {
        path.join(ARCHIVE_PREFIX)
    }
}

async fn process_input_to_directory(
    input: Uri,
    scrubbed: Option<bool>,
    auto_detect_filename: Option<&str>,
) -> (tempfile::TempDir, PathBuf) {
    let output = tempdir().expect("output tempdir");
    let output_path = output.path().to_path_buf();

    let receiver = Arc::new(
        Receiver::try_from_with_scrub(input, scrubbed, auto_detect_filename).expect("receiver"),
    );
    let exporter = Arc::new(
        Exporter::try_from(Uri::Directory(output_path.clone())).expect("exporter"),
    );

    let processor = Processor::try_new(receiver, exporter, Identifiers::default())
        .await
        .expect("processor ready");
    let processing = match processor.start().await {
        Ok(processing) => processing,
        Err(failed) => panic!("processor start failed: {}", failed.state.error),
    };
    match processing.process().await {
        Ok(_completed) => {}
        Err(failed) => panic!("processor process failed: {}", failed.state.error),
    };

    (output, output_path)
}

#[tokio::test]
async fn archive_export_normalizes_ips_with_stable_node_mapping() {
    let golden = manifest_dir().join(GOLDEN_ARCHIVE);
    if !golden.exists() {
        return;
    }

    let fixture_dir = tempdir().expect("fixture tempdir");
    let scrubbed_zip = fixture_dir.path().join("synthetic-malformed-ips-test.zip");
    let expectations = build_synthetic_scrubbed_archive(&scrubbed_zip);

    let (_output, output_path) =
        process_input_to_directory(Uri::File(scrubbed_zip), Some(true), None).await;
    assert_scrubbed_export(&output_path, &expectations);
}

#[tokio::test]
async fn directory_export_normalizes_ips_with_stable_node_mapping() {
    let golden = manifest_dir().join(GOLDEN_ARCHIVE);
    if !golden.exists() {
        return;
    }

    let fixture_dir = tempdir().expect("fixture tempdir");
    let scrubbed_zip = fixture_dir.path().join("synthetic-malformed-ips-test.zip");
    let expectations = build_synthetic_scrubbed_archive(&scrubbed_zip);

    let extract_dir = fixture_dir.path().join("synthetic-scrubbed-extracted");
    {
        let source_file = fs::File::open(&scrubbed_zip).expect("open synthetic zip");
        let mut archive = ZipArchive::new(source_file).expect("read synthetic zip");
        archive.extract(&extract_dir).expect("extract synthetic zip");
    }

    let input_dir = archive_root_path(&extract_dir);
    let (_output, output_path) =
        process_input_to_directory(Uri::Directory(input_dir), Some(true), None).await;
    assert_scrubbed_export(&output_path, &expectations);
}

#[tokio::test]
async fn directory_auto_detect_enables_scrub_when_path_contains_scrubbed() {
    let golden = manifest_dir().join(GOLDEN_ARCHIVE);
    if !golden.exists() {
        return;
    }

    let fixture_dir = tempdir().expect("fixture tempdir");
    let scrubbed_zip = fixture_dir.path().join("synthetic-malformed-ips-test.zip");
    let expectations = build_synthetic_scrubbed_archive(&scrubbed_zip);

    let extract_dir = fixture_dir.path().join("extracted-scrubbed-api-diagnostics");
    {
        let source_file = fs::File::open(&scrubbed_zip).expect("open synthetic zip");
        let mut archive = ZipArchive::new(source_file).expect("read synthetic zip");
        archive
            .extract(&extract_dir)
            .expect("extract synthetic zip");
    }

    let input_dir = archive_root_path(&extract_dir);
    let (_output, output_path) = process_input_to_directory(Uri::Directory(input_dir), None, None).await;
    assert_scrubbed_export(&output_path, &expectations);
}

#[tokio::test]
async fn directory_with_scrub_disabled_preserves_malformed_ips() {
    let golden = manifest_dir().join(GOLDEN_ARCHIVE);
    if !golden.exists() {
        return;
    }

    let fixture_dir = tempdir().expect("fixture tempdir");
    let scrubbed_zip = fixture_dir.path().join("synthetic-malformed-ips-test.zip");
    let expectations = build_synthetic_scrubbed_archive(&scrubbed_zip);
    let malformed = expectations
        .malformed_ips()
        .next()
        .expect("fixture malformed IP")
        .clone();

    let extract_dir = fixture_dir.path().join("synthetic-scrubbed-extracted");
    {
        let source_file = fs::File::open(&scrubbed_zip).expect("open synthetic zip");
        let mut archive = ZipArchive::new(source_file).expect("read synthetic zip");
        archive.extract(&extract_dir).expect("extract synthetic zip");
    }

    let input_dir = archive_root_path(&extract_dir);
    let (_output, output_path) =
        process_input_to_directory(Uri::Directory(input_dir), Some(false), None).await;
    assert_malformed_ips_preserved_in_node_metrics(&output_path, &malformed);
}

#[tokio::test]
async fn processes_non_scrubbed_golden_archive_without_error() {
    let archive = manifest_dir().join(GOLDEN_ARCHIVE);
    if !archive.exists() {
        return;
    }

    let (_output, output_path) =
        process_input_to_directory(Uri::File(archive), Some(false), None).await;
    let node_metrics = output_path.join("metrics-node-esdiag.ndjson");
    assert!(
        node_metrics.exists(),
        "expected metrics-node-esdiag.ndjson in {}",
        output_path.display()
    );

    let content = fs::read_to_string(node_metrics).expect("read node metrics");
    assert!(
        !content.trim().is_empty(),
        "golden archive should export node metrics"
    );
    assert!(content.contains("\"node\""));
}
