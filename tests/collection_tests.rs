use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_collect_minimal() {
    let dir = tempdir().unwrap();
    let out_dir = dir.path().to_str().unwrap();

    // This test expects a known host named "elasticsearch-local" to be configured in ~/.esdiag/hosts.yml
    // You can create this with: `esdiag host elasticsearch-local http://localhost:9200`
    let status = Command::new("cargo")
        .args(&[
            "run",
            "--",
            "collect",
            "elasticsearch-local",
            out_dir,
            "--type",
            "minimal",
        ])
        .status()
        .expect("Failed to execute process");

    assert!(status.success());

    // Find the generated zip file
    //

    // Note: Since esdiag compresses the output by default to an archive, we would normally use
    // zip extraction here, but because we are writing to a directory via DirectoryExporter,
    // the output is technically a directory named `api-diagnostics-*`.
    // Wait, the DirectoryExporter writes to a directory, let's just inspect it.

    let diag_dir = find_diag_dir(dir.path()).expect("Should have generated a directory");

    assert!(diag_dir.join("diagnostic_manifest.json").exists());
    assert!(diag_dir.join("version.json").exists()); // cluster api
    assert!(diag_dir.join("nodes.json").exists()); // nodes api
    assert!(diag_dir.join("cluster_settings.json").exists()); // from nodes resolving cluster_settings

    // The script `bin/min-diag.sh` collected a lot more, but our Rust `Minimal` currently just does `cluster` and `nodes` and `cluster_settings`.
    // Let's verify the manifest contains the correct collected_apis
    let manifest_content = fs::read_to_string(diag_dir.join("diagnostic_manifest.json")).unwrap();
    let manifest: Value = serde_json::from_str(&manifest_content).unwrap();

    let apis = manifest["collected_apis"].as_array().unwrap();
    let api_names: Vec<&str> = apis.iter().map(|v| v.as_str().unwrap()).collect();

    assert!(api_names.contains(&"cluster"));
    assert!(api_names.contains(&"nodes"));
    assert!(api_names.contains(&"cluster_settings"));
}

fn find_diag_dir(path: &Path) -> Option<std::path::PathBuf> {
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir()
            && entry
                .file_name()
                .to_str()
                .unwrap()
                .starts_with("api-diagnostics")
        {
            return Some(entry.path());
        }
    }
    None
}

#[test]
fn test_collect_light() {
    let dir = tempdir().unwrap();
    let out_dir = dir.path().to_str().unwrap();

    // This test expects a known host named "elasticsearch-local" to be configured in ~/.esdiag/hosts.yml
    // You can create this with: `esdiag host elasticsearch-local http://localhost:9200`
    let status = Command::new("cargo")
        .args(&[
            "run",
            "--",
            "collect",
            "elasticsearch-local",
            out_dir,
            "--include",
            "licenses,health_report,tasks",
            "--exclude",
            "nodes_stats,indices_stats,mapping_stats",
        ])
        .status()
        .expect("Failed to execute process");

    assert!(status.success());
    let diag_dir = find_diag_dir(dir.path()).expect("Should have generated a directory");

    assert!(diag_dir.join("diagnostic_manifest.json").exists());
    assert!(diag_dir.join("version.json").exists());
    assert!(diag_dir.join("licenses.json").exists());
    assert!(diag_dir.join("internal_health.json").exists());
    assert!(diag_dir.join("tasks.json").exists());

    assert!(!diag_dir.join("nodes_stats.json").exists());
    assert!(!diag_dir.join("indices_stats.json").exists());
    assert!(!diag_dir.join("mapping.json").exists());
}

#[test]
fn test_collect_support_all_endpoints() {
    let dir = tempdir().unwrap();
    let out_dir = dir.path().to_str().unwrap();

    // This test expects a known host named "elasticsearch-local" to be configured in ~/.esdiag/hosts.yml
    // You can create this with: `esdiag host elasticsearch-local http://localhost:9200`
    let status = Command::new("cargo")
        .args(&[
            "run",
            "--",
            "collect",
            "elasticsearch-local",
            out_dir,
            "--type",
            "support",
        ])
        .status()
        .expect("Failed to execute process");

    assert!(status.success());
    let diag_dir = find_diag_dir(dir.path()).expect("Should have generated a directory");

    assert!(diag_dir.join("diagnostic_manifest.json").exists());
    assert!(diag_dir.join("version.json").exists());
    assert!(diag_dir.join("nodes.json").exists());
    assert!(diag_dir.join("nodes_stats.json").exists());
    assert!(diag_dir.join("indices_stats.json").exists());
    assert!(diag_dir.join("cluster_settings.json").exists());
    assert!(diag_dir.join("licenses.json").exists());
}
