use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;
use zip::ZipArchive;

#[test]
fn test_collect_minimal() {
    let dir = tempdir().unwrap();
    let out_dir = dir.path().to_str().unwrap();

    // This test expects a known host named "elasticsearch-local" to be configured in ~/.esdiag/hosts.yml
    // You can create this with: `esdiag host elasticsearch-local http://localhost:9200`
    let status = Command::new("cargo")
        .args([
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
        .args([
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
        .args([
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

#[test]
fn test_collect_zip_writes_archive() {
    let dir = tempdir().unwrap();
    let out_dir = dir.path().to_str().unwrap();

    let status = Command::new("cargo")
        .args([
            "run",
            "--",
            "collect",
            "elasticsearch-local",
            "--zip",
            out_dir,
            "--type",
            "minimal",
        ])
        .status()
        .expect("Failed to execute process");

    assert!(status.success());
    let zip_path = find_diag_zip(dir.path()).expect("Should have generated a diagnostic zip");
    let file = fs::File::open(zip_path).expect("zip exists");
    let mut archive = ZipArchive::new(file).expect("zip is valid");
    assert!(archive.by_name("diagnostic_manifest.json").is_ok());
    assert!(archive.by_name("version.json").is_ok());
    assert!(archive.by_name("nodes.json").is_ok());
}

#[test]
fn test_process_zip_writes_diagnostic_archive() {
    let dir = tempdir().unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .current_dir(dir.path())
        .args(["process", "elasticsearch-local", "-", "--zip"])
        .status()
        .expect("Failed to execute process");

    // `process` may fail depending on local cluster shape, but `--zip` must still emit the archive.
    assert!(status.code().is_some());
    let zip_path = find_diag_zip(dir.path()).expect("Should have generated a diagnostic zip");
    let file = fs::File::open(zip_path).expect("zip exists");
    let mut archive = ZipArchive::new(file).expect("zip is valid");
    assert!(archive.by_name("diagnostic_manifest.json").is_ok());
    assert!(archive.by_name("version.json").is_ok());
}

fn find_diag_zip(path: &Path) -> Option<std::path::PathBuf> {
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if entry.file_type().unwrap().is_file()
            && file_name.starts_with("api-diagnostics-")
            && file_name.ends_with(".zip")
        {
            return Some(entry.path());
        }
    }
    None
}

fn file_set(root: &Path) -> BTreeSet<String> {
    fn visit(dir: &Path, root: &Path, out: &mut BTreeSet<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                visit(&path, root, out);
            } else if entry.file_type().unwrap().is_file() {
                out.insert(path.strip_prefix(root).unwrap().to_string_lossy().to_string());
            }
        }
    }

    let mut files = BTreeSet::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn test_collect_zip_matches_directory_file_set_light_and_support() {
    for variant in ["light", "support"] {
        let dir = tempdir().unwrap();
        let base = dir.path();
        let dir_out = base.join("dir");
        let zip_out = base.join("zip");
        let extract_out = base.join("extract");
        fs::create_dir_all(&dir_out).unwrap();
        fs::create_dir_all(&zip_out).unwrap();
        fs::create_dir_all(&extract_out).unwrap();

        let status_dir = Command::new(env!("CARGO_BIN_EXE_esdiag"))
            .args([
                "collect",
                "localhost",
                dir_out.to_str().unwrap(),
                "--type",
                variant,
            ])
            .status()
            .expect("Failed to execute directory collect");
        assert!(status_dir.success());

        let status_zip = Command::new(env!("CARGO_BIN_EXE_esdiag"))
            .args([
                "collect",
                "localhost",
                zip_out.to_str().unwrap(),
                "--zip",
                "--type",
                variant,
            ])
            .status()
            .expect("Failed to execute zip collect");
        assert!(status_zip.success());

        let diag_dir = find_diag_dir(&dir_out).expect("missing diagnostic directory output");
        let zip_path = find_diag_zip(&zip_out).expect("missing zip output");

        let file = fs::File::open(zip_path).expect("zip exists");
        let mut archive = ZipArchive::new(file).expect("zip is valid");
        archive.extract(&extract_out).expect("zip extraction succeeds");

        let dir_set = file_set(&diag_dir);
        let zip_set = file_set(&extract_out);
        assert_eq!(dir_set, zip_set, "file set mismatch for variant {variant}");
    }
}

#[test]
fn test_collect_support_zip_repeated_has_no_duplicate_entry_errors() {
    let dir = tempdir().unwrap();
    let out_dir = dir.path().to_str().unwrap();

    for _ in 0..3 {
        let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
            .args(["collect", "localhost", out_dir, "--zip", "--type", "support"])
            .output()
            .expect("Failed to execute support zip collect");
        assert!(output.status.success());

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("Duplicate filename"),
            "zip collection reported duplicate entry: {stderr}"
        );
    }
}
