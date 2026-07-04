use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{TempDir, tempdir};

fn logstash_archive_fixtures() -> Vec<PathBuf> {
    let archives_dir = Path::new("tests/archives");
    let mut archives = Vec::new();
    for entry in fs::read_dir(archives_dir).expect("read tests/archives") {
        let entry = entry.expect("archive dir entry");
        let path = entry.path();
        let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
        if filename.starts_with("logstash-api-diagnostics-") && filename.ends_with(".zip") {
            archives.push(path);
        }
    }
    archives.sort();
    assert!(!archives.is_empty(), "no logstash archive fixtures found");
    archives
}

fn process_archive(archive: &Path, output: &Path, home: &Path) {
    let process = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args([
            "process",
            archive.to_str().expect("archive path"),
            output.to_str().expect("output path"),
        ])
        .env("HOME", home)
        .env("USERPROFILE", home)
        .output()
        .expect("run esdiag process");

    assert!(
        process.status.success(),
        "process failed for {}\nstdout:\n{}\nstderr:\n{}",
        archive.display(),
        String::from_utf8_lossy(&process.stdout),
        String::from_utf8_lossy(&process.stderr)
    );
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

fn assert_has_dataset(docs: &[Value], expected_dataset: &str, archive: &Path, stream: &str) {
    assert!(
        docs.iter()
            .any(|doc| doc["data_stream"]["dataset"].as_str() == Some(expected_dataset)),
        "{} in {} did not contain data_stream.dataset={}",
        stream,
        archive.display(),
        expected_dataset
    );
}

fn assert_no_base_logstash_dataset(docs: &[Value], archive: &Path, stream: &str) {
    assert!(
        docs.iter()
            .all(|doc| doc["data_stream"]["dataset"].as_str() != Some("logstash")),
        "{} in {} unexpectedly used broad data_stream.dataset=logstash",
        stream,
        archive.display()
    );
}

#[test]
fn logstash_archive_fixtures_process_across_supported_versions() {
    let home = tempdir().expect("temp HOME");
    let output_root = TempDir::new().expect("temp output");

    for archive in logstash_archive_fixtures() {
        let archive_output = output_root.path().join(
            archive
                .file_stem()
                .and_then(|name| name.to_str())
                .expect("archive file stem"),
        );
        fs::create_dir(&archive_output).expect("archive output dir");
        process_archive(&archive, &archive_output, home.path());

        let settings_node = read_docs(&archive_output, "settings-logstash.node-esdiag.ndjson");
        let metrics_node = read_docs(&archive_output, "metrics-logstash.node-esdiag.ndjson");
        let settings_plugin = read_docs(&archive_output, "settings-logstash.plugin-esdiag.ndjson");

        assert_has_dataset(
            &settings_node,
            "logstash.node",
            &archive,
            "settings-logstash.node-esdiag",
        );
        assert_has_dataset(
            &settings_node,
            "logstash.pipeline",
            &archive,
            "settings-logstash.node-esdiag",
        );
        assert_has_dataset(&metrics_node, "logstash.node", &archive, "metrics-logstash.node-esdiag");
        assert_has_dataset(
            &metrics_node,
            "logstash.pipeline",
            &archive,
            "metrics-logstash.node-esdiag",
        );
        assert_has_dataset(
            &metrics_node,
            "logstash.plugin",
            &archive,
            "metrics-logstash.node-esdiag",
        );
        assert_has_dataset(
            &settings_plugin,
            "logstash.plugin",
            &archive,
            "settings-logstash.plugin-esdiag",
        );
        assert_no_base_logstash_dataset(&settings_node, &archive, "settings-logstash.node-esdiag");
        assert_no_base_logstash_dataset(&metrics_node, &archive, "metrics-logstash.node-esdiag");
        assert_no_base_logstash_dataset(&settings_plugin, &archive, "settings-logstash.plugin-esdiag");
    }
}
