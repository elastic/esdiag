mod scrub_normalization_assertions;

use esdiag::{
    data::Uri,
    exporter::DocumentExporter,
    job::{
        context::ExecutionContext,
        executor::execute_with_context,
        model::{BindingKey, ExportTarget, Input, Job, Process},
    },
    processor::Identifiers,
    receiver::Receiver,
};
use scrub_normalization_assertions::{
    ScrubFixtureExpectations, assert_malformed_ips_preserved_in_node_metrics, assert_scrubbed_export,
    ensure_two_nodes_in_nodes_json, ensure_two_nodes_in_nodes_stats_json, ensure_two_nodes_in_tasks_json,
    inject_malformed_ips_in_nodes_json, inject_malformed_ips_in_nodes_stats_json, inject_malformed_ips_in_tasks_json,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tempfile::tempdir;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const GOLDEN_ARCHIVE: &str = "tests/archives/elasticsearch-api-diagnostics-9.3.3.zip";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn golden_archive() -> PathBuf {
    let path = manifest_dir().join(GOLDEN_ARCHIVE);
    assert!(path.exists(), "missing test fixture: {}", path.display());
    path
}

fn build_synthetic_scrubbed_archive(out_zip: &Path) -> ScrubFixtureExpectations {
    build_synthetic_scrubbed_archive_with_address_variants(out_zip, false)
}

fn build_synthetic_scrubbed_archive_with_address_variants(
    out_zip: &Path,
    address_variants: bool,
) -> ScrubFixtureExpectations {
    let golden = golden_archive();
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
        let mut content = String::new();
        source_archive
            .by_name(entry)
            .unwrap_or_else(|_| panic!("missing {entry}"))
            .read_to_string(&mut content)
            .expect("read entry");

        content = match entry {
            "nodes.json" => {
                let content = ensure_two_nodes_in_nodes_json(&content);
                let (updated, exp) = inject_malformed_ips_in_nodes_json(&content);
                expectations = Some(exp);
                if address_variants {
                    vary_node_addresses(entry, &updated)
                } else {
                    updated
                }
            }
            "nodes_stats.json" => {
                let content = ensure_two_nodes_in_nodes_stats_json(&content);
                let exp = expectations
                    .as_ref()
                    .expect("nodes.json must be processed before nodes_stats.json");
                let updated = inject_malformed_ips_in_nodes_stats_json(&content, exp);
                if address_variants {
                    vary_node_addresses(entry, &updated)
                } else {
                    updated
                }
            }
            "tasks.json" => {
                let content = ensure_two_nodes_in_tasks_json(&content);
                let exp = expectations
                    .as_ref()
                    .expect("nodes.json must be processed before tasks.json");
                let updated = inject_malformed_ips_in_tasks_json(&content, exp);
                if address_variants {
                    vary_node_addresses(entry, &updated)
                } else {
                    updated
                }
            }
            _ => content,
        };

        dest_archive.start_file(entry, options).expect("start zip entry");
        dest_archive.write_all(content.as_bytes()).expect("write zip entry");
    }

    dest_archive.finish().expect("finish synthetic scrubbed zip");
    expectations.expect("nodes.json expectations")
}

fn vary_node_addresses(entry: &str, content: &str) -> String {
    const ADDRESSES: [&str; 2] = ["18446744073709551616.255.256.1", "192.0.2.50"];
    let mut value: Value = serde_json::from_str(content).expect("parse synthetic nodes");
    let nodes = value["nodes"].as_object_mut().expect("synthetic nodes object");
    let mut ids: Vec<_> = nodes.keys().cloned().collect();
    assert_eq!(
        ids.len(),
        ADDRESSES.len(),
        "synthetic address variants require two nodes"
    );
    ids.sort();
    for (index, id) in ids.iter().enumerate() {
        let address = ADDRESSES[index];
        let node = nodes.get_mut(id).expect("sorted node id");
        node["ip"] = json!(format!("{address}:9300"));
        node["host"] = json!(format!("{address}:9300"));
        node["transport_address"] = json!(format!("{address}:9300"));
        if entry == "nodes.json" {
            node["name"] = json!(address);
            node["settings"]["network"]["publish_host"] = json!(format!("{address}:9300"));
            node["transport"]["publish_address"] = json!(format!("{address}:9300"));
        }
    }
    serde_json::to_string(&value).expect("serialize varied nodes")
}

async fn process_input_to_directory(
    input: Uri,
    scrubbed: Option<bool>,
    auto_detect_filename: Option<&str>,
) -> (tempfile::TempDir, PathBuf) {
    let output = tempdir().expect("output tempdir");
    let output_path = output.path().to_path_buf();

    let receiver = Receiver::try_from_with_scrub(input, scrubbed, auto_detect_filename).expect("receiver");
    let input_binding = BindingKey::try_new("scrub-test-input").expect("input binding");
    let output_binding = BindingKey::try_new("scrub-test-output").expect("output binding");
    let mut context = ExecutionContext::default();
    context.inputs.bind_receiver(input_binding.clone(), receiver, None);
    context.bind_document_exporter(
        output_binding.clone(),
        DocumentExporter::try_from(Uri::Directory(output_path.clone())).expect("exporter"),
    );
    let job = Job::try_new(
        Identifiers::default(),
        Input::LoadBinding { binding: input_binding },
        None,
        Some(Process {
            selection: None,
            export: ExportTarget::Binding {
                binding: output_binding,
            },
        }),
        None,
    )
    .expect("processing job");
    let outcome = execute_with_context(job, context).await;
    assert!(outcome.succeeded(), "processing failed: {:?}", outcome.stages);

    (output, output_path)
}

#[tokio::test]
async fn archive_export_normalizes_ips_with_stable_node_mapping() {
    let fixture_dir = tempdir().expect("fixture tempdir");
    let scrubbed_zip = fixture_dir.path().join("synthetic-malformed-ips-test.zip");
    let expectations = build_synthetic_scrubbed_archive(&scrubbed_zip);

    let (_output, output_path) = process_input_to_directory(Uri::File(scrubbed_zip), Some(true), None).await;
    assert_scrubbed_export(&output_path, &expectations);
}

#[tokio::test]
async fn directory_export_normalizes_ips_with_stable_node_mapping() {
    let fixture_dir = tempdir().expect("fixture tempdir");
    let scrubbed_zip = fixture_dir.path().join("synthetic-malformed-ips-test.zip");
    let expectations = build_synthetic_scrubbed_archive(&scrubbed_zip);

    let extract_dir = fixture_dir.path().join("synthetic-scrubbed-extracted");
    {
        let source_file = fs::File::open(&scrubbed_zip).expect("open synthetic zip");
        let mut archive = ZipArchive::new(source_file).expect("read synthetic zip");
        archive.extract(&extract_dir).expect("extract synthetic zip");
    }

    let (_output, output_path) = process_input_to_directory(Uri::Directory(extract_dir), Some(true), None).await;
    assert_scrubbed_export(&output_path, &expectations);
}

#[tokio::test]
async fn directory_auto_detect_enables_scrub_when_path_contains_scrubbed() {
    let fixture_dir = tempdir().expect("fixture tempdir");
    let scrubbed_zip = fixture_dir.path().join("synthetic-malformed-ips-test.zip");
    let expectations = build_synthetic_scrubbed_archive(&scrubbed_zip);

    let extract_dir = fixture_dir.path().join("extracted-scrubbed-api-diagnostics");
    {
        let source_file = fs::File::open(&scrubbed_zip).expect("open synthetic zip");
        let mut archive = ZipArchive::new(source_file).expect("read synthetic zip");
        archive.extract(&extract_dir).expect("extract synthetic zip");
    }

    let (_output, output_path) = process_input_to_directory(Uri::Directory(extract_dir), None, None).await;
    assert_scrubbed_export(&output_path, &expectations);
}

#[tokio::test]
async fn directory_with_scrub_disabled_preserves_malformed_ips() {
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

    let (_output, output_path) = process_input_to_directory(Uri::Directory(extract_dir), Some(false), None).await;
    assert_malformed_ips_preserved_in_node_metrics(&output_path, &malformed);
}

#[tokio::test]
async fn archive_and_directory_exports_preserve_address_semantics_and_unrelated_names() {
    const ORIGINAL: [&str; 2] = ["18446744073709551616.255.256.1", "192.0.2.50"];
    const NORMALIZED: [&str; 2] = ["1.0.1.1", "192.0.2.50"];
    let fixture = tempdir().expect("fixture tempdir");
    let archive_path = fixture.path().join("synthetic-address-variants.zip");
    let expectations = build_synthetic_scrubbed_archive_with_address_variants(&archive_path, true);
    let mut ids: Vec<_> = expectations.normalized_by_node_id.keys().cloned().collect();
    ids.sort();
    assert_eq!(ids.len(), 2);

    let extract_dir = fixture.path().join("extracted-address-variants");
    let mut zip = ZipArchive::new(fs::File::open(&archive_path).expect("open variants archive")).expect("read zip");
    zip.extract(&extract_dir).expect("extract variants");

    for input in [Uri::File(archive_path), Uri::Directory(extract_dir)] {
        for scrubbed in [true, false] {
            let (_output, output_path) = process_input_to_directory(input.clone(), Some(scrubbed), None).await;
            let documents =
                fs::read_to_string(output_path.join("metrics-node-esdiag.ndjson")).expect("node metrics export");
            let mut seen = std::collections::HashSet::new();
            for line in documents.lines().filter(|line| !line.is_empty()) {
                let doc: Value = serde_json::from_str(line).expect("node metrics document");
                let node = &doc["node"];
                let id = node["id"].as_str().expect("node id");
                let index = ids
                    .iter()
                    .position(|expected| expected == id)
                    .expect("synthetic node id");
                seen.insert(id.to_owned());
                let address = if scrubbed { NORMALIZED[index] } else { ORIGINAL[index] };
                let pure = if scrubbed {
                    address.to_string()
                } else {
                    format!("{address}:9300")
                };
                assert_eq!(node["ip"].as_str(), Some(pure.as_str()));
                assert_eq!(node["host"].as_str(), Some(pure.as_str()));
                assert_eq!(
                    node["transport_address"].as_str(),
                    Some(format!("{address}:9300").as_str())
                );
                assert_eq!(
                    node["name"].as_str(),
                    Some(ORIGINAL[index]),
                    "non-address name is unchanged"
                );
            }
            assert_eq!(seen.len(), ids.len(), "export all synthetic nodes");

            let settings =
                fs::read_to_string(output_path.join("settings-node-esdiag.ndjson")).expect("node settings export");
            for line in settings.lines().filter(|line| !line.is_empty()) {
                let doc: Value = serde_json::from_str(line).expect("node settings document");
                let node = &doc["node"];
                let id = node["id"].as_str().expect("settings node id");
                let index = ids
                    .iter()
                    .position(|expected| expected == id)
                    .expect("synthetic settings node");
                let expected = if scrubbed {
                    NORMALIZED[index].to_string()
                } else {
                    format!("{}:9300", ORIGINAL[index])
                };
                assert_eq!(
                    node["settings"]["network"]["publish_host"].as_str(),
                    Some(expected.as_str())
                );
            }
        }
    }
}

#[tokio::test]
async fn processes_non_scrubbed_golden_archive_without_error() {
    let archive = golden_archive();

    let (_output, output_path) = process_input_to_directory(Uri::File(archive), Some(false), None).await;
    let node_metrics = output_path.join("metrics-node-esdiag.ndjson");
    assert!(
        node_metrics.exists(),
        "expected metrics-node-esdiag.ndjson in {}",
        output_path.display()
    );

    let content = fs::read_to_string(node_metrics).expect("read node metrics");
    assert!(!content.trim().is_empty(), "golden archive should export node metrics");
    assert!(content.contains("\"node\""));
}
