use axum::{
    Json, Router,
    extract::{Path as AxumPath, Query},
    routing::get,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread::sleep;
use std::time::{Duration, Instant};
use tempfile::tempdir;
use uuid::Uuid;
use zip::ZipArchive;

struct ExtractedDiag {
    _tmp: tempfile::TempDir,
    path: PathBuf,
}

#[test]
#[ignore = "requires configured elasticsearch-local host and a running Elasticsearch instance"]
fn test_collect_minimal() {
    let dir = tempdir().unwrap();
    let out_dir = dir.path().to_str().unwrap();

    // This test expects a known host named "elasticsearch-local" to be configured in ~/.esdiag/hosts.yml
    // You can create this with: `esdiag host add elasticsearch-local elasticsearch http://localhost:9200`
    let status = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["collect", "elasticsearch-local", out_dir, "--type", "minimal"])
        .status()
        .expect("Failed to execute process");

    assert!(status.success());

    let extracted = extract_diag_zip_to_temp(dir.path()).expect("Should have generated a zip");

    assert!(extracted.path.join("diagnostic_manifest.json").exists());
    assert!(extracted.path.join("version.json").exists()); // cluster api
    assert!(extracted.path.join("nodes.json").exists()); // nodes api
    assert!(extracted.path.join("cluster_settings.json").exists()); // from nodes resolving cluster_settings

    // The script `bin/min-diag.sh` collected a lot more, but our Rust `Minimal` currently just does
    // `cluster`, `nodes`, and `cluster_settings`. Verify the manifest records those requested APIs.
    let manifest_content = fs::read_to_string(extracted.path.join("diagnostic_manifest.json")).unwrap();
    let manifest: Value = serde_json::from_str(&manifest_content).unwrap();

    let api_names = requested_api_names(&manifest);

    assert!(api_names.contains(&"cluster"));
    assert!(api_names.contains(&"nodes"));
    assert!(api_names.contains(&"cluster_settings"));
    assert_eq!(requested_api_status(&manifest, "cluster"), Some(200));
    assert_eq!(requested_api_status(&manifest, "nodes"), Some(200));
    assert_eq!(requested_api_status(&manifest, "cluster_settings"), Some(200));
    assert_eq!(requested_api_retries(&manifest, "cluster"), Some(0));
    assert!(requested_api_response_time_ms(&manifest, "cluster").is_some());
    assert!(requested_api_response_size_bytes(&manifest, "cluster").is_some());
}

fn extract_diag_zip_to_temp(path: &Path) -> Option<ExtractedDiag> {
    let zip_path = find_diag_zip(path)?;
    let extract_root = tempfile::tempdir().ok()?;
    let extract_path = extract_root.path().to_path_buf();
    let file = fs::File::open(zip_path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    archive.extract(&extract_path).ok()?;
    Some(ExtractedDiag {
        _tmp: extract_root,
        path: extract_path,
    })
}

fn find_diag_dir(path: &Path) -> Option<std::path::PathBuf> {
    for entry in fs::read_dir(path).ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let valid_prefix = name.starts_with("api-diagnostics-") || name.starts_with("api-diagnostics");
        if entry.file_type().ok()?.is_dir() && valid_prefix {
            return Some(entry.path());
        }
    }
    None
}

#[test]
#[ignore = "requires configured elasticsearch-local host and a running Elasticsearch instance"]
fn test_collect_light() {
    let dir = tempdir().unwrap();
    let out_dir = dir.path().to_str().unwrap();

    // This test expects a known host named "elasticsearch-local" to be configured in ~/.esdiag/hosts.yml
    // You can create this with: `esdiag host add elasticsearch-local elasticsearch http://localhost:9200`
    let status = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args([
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
    let extracted = extract_diag_zip_to_temp(dir.path()).expect("Should have generated a zip");

    assert!(extracted.path.join("diagnostic_manifest.json").exists());
    assert!(extracted.path.join("version.json").exists());
    assert!(extracted.path.join("licenses.json").exists());
    assert!(extracted.path.join("internal_health.json").exists());
    assert!(extracted.path.join("tasks.json").exists());

    assert!(!extracted.path.join("nodes_stats.json").exists());
    assert!(!extracted.path.join("indices_stats.json").exists());
    assert!(!extracted.path.join("mapping.json").exists());
}

#[test]
#[ignore = "requires configured elasticsearch-local host and a running Elasticsearch instance"]
fn test_collect_support_all_endpoints() {
    let dir = tempdir().unwrap();
    let out_dir = dir.path().to_str().unwrap();

    // This test expects a known host named "elasticsearch-local" to be configured in ~/.esdiag/hosts.yml
    // You can create this with: `esdiag host add elasticsearch-local elasticsearch http://localhost:9200`
    let status = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["collect", "elasticsearch-local", out_dir, "--type", "support"])
        .status()
        .expect("Failed to execute process");

    assert!(status.success());
    let extracted = extract_diag_zip_to_temp(dir.path()).expect("Should have generated a zip");

    assert!(extracted.path.join("diagnostic_manifest.json").exists());
    assert!(extracted.path.join("version.json").exists());
    assert!(extracted.path.join("nodes.json").exists());
    assert!(extracted.path.join("nodes_stats.json").exists());
    assert!(extracted.path.join("indices_stats.json").exists());
    assert!(extracted.path.join("cluster_settings.json").exists());
    assert!(extracted.path.join("licenses.json").exists());
}

#[test]
#[ignore = "requires configured elasticsearch-local host and a running Elasticsearch instance"]
fn test_collect_zip_writes_archive() {
    let dir = tempdir().unwrap();
    let out_dir = dir.path().to_str().unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["collect", "elasticsearch-local", out_dir, "--type", "minimal"])
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
#[ignore = "requires configured elasticsearch-local host and a running Elasticsearch instance"]
fn test_process_command_returns_status() {
    let status = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["process", "elasticsearch-local", "-"])
        .status()
        .expect("Failed to execute process");

    // `process` may fail depending on local cluster shape, but should not crash.
    assert!(status.code().is_some());
}

fn find_diag_zip(path: &Path) -> Option<std::path::PathBuf> {
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let valid_prefix = file_name.starts_with("api-diagnostics-") || file_name.contains("-api-diagnostics-");
        if entry.file_type().unwrap().is_file() && valid_prefix && file_name.ends_with(".zip") {
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

fn requested_api_names(manifest: &Value) -> Vec<&str> {
    manifest["requested_apis"]
        .as_object()
        .expect("requested_apis object")
        .keys()
        .map(String::as_str)
        .collect()
}

fn requested_api_status(manifest: &Value, name: &str) -> Option<u64> {
    manifest["requested_apis"]
        .as_object()
        .expect("requested_apis object")
        .get(name)
        .and_then(|value| value["status"].as_u64())
}

fn requested_api_response_time_ms(manifest: &Value, name: &str) -> Option<u64> {
    manifest["requested_apis"]
        .as_object()
        .expect("requested_apis object")
        .get(name)
        .and_then(|value| value["response_time_ms"].as_u64())
}

fn requested_api_retries(manifest: &Value, name: &str) -> Option<u64> {
    manifest["requested_apis"]
        .as_object()
        .expect("requested_apis object")
        .get(name)
        .and_then(|value| value["retries"].as_u64())
}

fn requested_api_response_size_bytes(manifest: &Value, name: &str) -> Option<u64> {
    manifest["requested_apis"]
        .as_object()
        .expect("requested_apis object")
        .get(name)
        .and_then(|value| value["response_size_bytes"].as_u64())
}

#[test]
#[ignore = "requires configured elasticsearch-local host and a running Elasticsearch instance"]
fn test_collect_zip_matches_directory_file_set_light_and_support() {
    for variant in ["light", "support"] {
        let dir = tempdir().unwrap();
        let base = dir.path();
        let zip_out_a = base.join("zip-a");
        let zip_out_b = base.join("zip-b");
        let extract_out = base.join("extract");
        fs::create_dir_all(&zip_out_a).unwrap();
        fs::create_dir_all(&zip_out_b).unwrap();
        fs::create_dir_all(&extract_out).unwrap();

        let status_a = Command::new(env!("CARGO_BIN_EXE_esdiag"))
            .args([
                "collect",
                "elasticsearch-local",
                zip_out_a.to_str().unwrap(),
                "--type",
                variant,
            ])
            .status()
            .expect("Failed to execute first collect");
        assert!(status_a.success());

        let status_b = Command::new(env!("CARGO_BIN_EXE_esdiag"))
            .args([
                "collect",
                "elasticsearch-local",
                zip_out_b.to_str().unwrap(),
                "--type",
                variant,
            ])
            .status()
            .expect("Failed to execute second collect");
        assert!(status_b.success());

        let zip_path_a = find_diag_zip(&zip_out_a).expect("missing first zip output");
        let zip_path_b = find_diag_zip(&zip_out_b).expect("missing second zip output");

        let file = fs::File::open(zip_path_b).expect("zip exists");
        let mut archive = ZipArchive::new(file).expect("zip is valid");
        archive.extract(&extract_out).expect("zip extraction succeeds");

        let file = fs::File::open(zip_path_a).expect("zip exists");
        let mut archive = ZipArchive::new(file).expect("zip is valid");
        let extract_a = base.join("extract-a");
        fs::create_dir_all(&extract_a).unwrap();
        archive.extract(&extract_a).expect("zip extraction succeeds");

        let dir_set = file_set(&extract_a);
        let zip_set = file_set(&extract_out);
        assert_eq!(dir_set, zip_set, "file set mismatch for variant {variant}");
    }
}

#[test]
#[ignore = "requires configured elasticsearch-local host and a running Elasticsearch instance"]
fn test_collect_support_zip_repeated_has_no_duplicate_entry_errors() {
    let dir = tempdir().unwrap();
    let out_dir = dir.path().to_str().unwrap();

    for _ in 0..3 {
        let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
            .args(["collect", "elasticsearch-local", out_dir, "--type", "support"])
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

#[test]
#[ignore = "requires configured elasticsearch-local host and a running Elasticsearch instance"]
fn test_collect_zip_accepts_output_directory_with_dot() {
    let dir = tempdir().unwrap();
    let dotted_out = dir.path().join("out.v1");

    let status = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args([
            "collect",
            "elasticsearch-local",
            dotted_out.to_str().unwrap(),
            "--type",
            "minimal",
        ])
        .status()
        .expect("Failed to execute collect with dotted output directory");

    assert!(status.success());
    let zip_path = find_diag_zip(&dotted_out).expect("missing zip output in dotted directory");
    assert!(zip_path.exists());
}

#[test]
#[ignore = "requires configured elasticsearch-local host and a running Elasticsearch instance"]
fn test_process_zip_accepts_output_directory_with_dot() {
    let dir = tempdir().unwrap();
    let dotted_out = dir.path().join("out.v1.ndjson");

    let status = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["process", "elasticsearch-local", dotted_out.to_str().unwrap()])
        .status()
        .expect("Failed to execute process with dotted output file");

    // Process may fail on local cluster contents, but should not crash.
    assert!(status.code().is_some());
}

#[test]
fn test_collect_rejects_hosts_without_collect_role() {
    let home = tempdir().expect("temp home");
    let hosts_path = home.path().join("hosts.yml");
    fs::write(
        &hosts_path,
        r#"send-only:
  auth: NoAuth
  app: elasticsearch
  roles:
    - send
  url: http://localhost:9200
"#,
    )
    .expect("write hosts");

    let out = tempdir().expect("out");
    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args([
            "collect",
            "send-only",
            out.path().to_str().expect("out path"),
            "--type",
            "minimal",
        ])
        .env("ESDIAG_HOSTS", &hosts_path)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .output()
        .expect("run collect");

    assert!(
        !output.status.success(),
        "collect should fail for host without collect role"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("required role 'collect'"));
}

#[test]
fn test_process_rejects_output_host_without_send_role() {
    let home = tempdir().expect("temp home");
    let hosts_path = home.path().join("hosts.yml");
    fs::write(
        &hosts_path,
        r#"collect-source:
  auth: NoAuth
  app: elasticsearch
  roles:
    - collect
  url: http://localhost:9200
collect-only-output:
  auth: NoAuth
  app: elasticsearch
  roles:
    - collect
  url: http://localhost:9201
"#,
    )
    .expect("write hosts");

    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["process", "collect-source", "collect-only-output"])
        .env("ESDIAG_HOSTS", &hosts_path)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .output()
        .expect("run process");

    assert!(
        !output.status.success(),
        "process should fail when output host has no send role"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("required role 'send'"));
}

#[test]
#[ignore = "requires docker or podman runtime"]
fn test_collect_no_auth_with_ephemeral_container() {
    let runtime = match container_runtime() {
        Some(runtime) => runtime,
        None => {
            eprintln!("Skipping: no docker/podman runtime available");
            return;
        }
    };

    let image = elastic_image();
    let port = portpicker::pick_unused_port().expect("free port");
    let _container = RunningEsContainer::start(&runtime, &image, port, SecurityMode::Disabled, None)
        .expect("start insecure elasticsearch container");

    wait_for_es(port, SecurityMode::Disabled, None).expect("insecure Elasticsearch should be ready");

    let test_home = tempdir().expect("temp home");
    let host_name = "it-noauth";
    let host_url = format!("http://127.0.0.1:{port}");

    let host = run_esdiag(
        &[
            "host",
            "add",
            host_name,
            "elasticsearch",
            &host_url,
            "--accept-invalid-certs",
            "true",
        ],
        &test_home,
        &[],
    );
    assert_success(&host, "host no-auth");

    let collect_out = tempdir().expect("collect output");
    let collect = run_esdiag(
        &[
            "collect",
            host_name,
            collect_out.path().to_str().expect("path"),
            "--type",
            "minimal",
        ],
        &test_home,
        &[],
    );
    assert_success(&collect, "collect no-auth");
    assert_has_diagnostic_manifest(collect_out.path(), test_home.path(), "collect no-auth", &collect);
}

#[test]
#[ignore = "requires docker or podman runtime"]
fn test_collect_with_plaintext_and_keystore_auth_modes() {
    let runtime = match container_runtime() {
        Some(runtime) => runtime,
        None => {
            eprintln!("Skipping: no docker/podman runtime available");
            return;
        }
    };

    let image = elastic_image();
    let port = portpicker::pick_unused_port().expect("free port");
    let elastic_password = "esdiag-it-pass";
    let _container = RunningEsContainer::start(&runtime, &image, port, SecurityMode::Enabled, Some(elastic_password))
        .expect("start secure elasticsearch container");

    wait_for_es(port, SecurityMode::Enabled, Some(elastic_password)).expect("secure Elasticsearch should be ready");
    let api_key = create_api_key(port, elastic_password).expect("create api key");

    let test_home = tempdir().expect("temp home");
    let keystore_password = "esdiag-keystore-password";
    let host_url = format!("http://127.0.0.1:{port}");

    // 1) collect with basic auth saved directly in hosts.yml
    let basic_plain_name = "it-basic-plain";
    let host_plain = run_esdiag(
        &[
            "host",
            "add",
            basic_plain_name,
            "elasticsearch",
            &host_url,
            "--user",
            "elastic",
            "--password",
            elastic_password,
            "--accept-invalid-certs",
            "true",
        ],
        &test_home,
        &[],
    );
    assert_success(&host_plain, "host basic plaintext");
    run_collect_assert_ok(&test_home, basic_plain_name, "collect basic plaintext");

    // 2) collect with basic auth loaded from keystore
    let add_basic_secret = run_esdiag(
        &[
            "keystore",
            "add",
            "it-basic-secret",
            "--user",
            "elastic",
            "--password",
            elastic_password,
        ],
        &test_home,
        &[("ESDIAG_KEYSTORE_PASSWORD", keystore_password)],
    );
    assert_success(&add_basic_secret, "keystore add basic");

    let host_basic_keystore = run_esdiag(
        &[
            "host",
            "add",
            "it-basic-keystore",
            "elasticsearch",
            &host_url,
            "--secret",
            "it-basic-secret",
            "--accept-invalid-certs",
            "true",
        ],
        &test_home,
        &[("ESDIAG_KEYSTORE_PASSWORD", keystore_password)],
    );
    assert_success(&host_basic_keystore, "host basic keystore");
    run_collect_assert_ok_with_env(
        &test_home,
        "it-basic-keystore",
        &[("ESDIAG_KEYSTORE_PASSWORD", keystore_password)],
        "collect basic keystore",
    );

    // 3) collect with apikey stored directly in hosts.yml
    let apikey_plain_name = "it-apikey-plain";
    let host_apikey_plain = run_esdiag(
        &[
            "host",
            "add",
            apikey_plain_name,
            "elasticsearch",
            &host_url,
            "--apikey",
            &api_key,
            "--accept-invalid-certs",
            "true",
        ],
        &test_home,
        &[],
    );
    assert_success(&host_apikey_plain, "host apikey plaintext");
    run_collect_assert_ok(&test_home, apikey_plain_name, "collect apikey plaintext");

    // 4) collect with apikey loaded from keystore
    let add_apikey_secret = run_esdiag(
        &["keystore", "add", "it-apikey-secret", "--apikey", &api_key],
        &test_home,
        &[("ESDIAG_KEYSTORE_PASSWORD", keystore_password)],
    );
    assert_success(&add_apikey_secret, "keystore add apikey");

    let host_apikey_keystore = run_esdiag(
        &[
            "host",
            "add",
            "it-apikey-keystore",
            "elasticsearch",
            &host_url,
            "--secret",
            "it-apikey-secret",
            "--accept-invalid-certs",
            "true",
        ],
        &test_home,
        &[("ESDIAG_KEYSTORE_PASSWORD", keystore_password)],
    );
    assert_success(&host_apikey_keystore, "host apikey keystore");
    run_collect_assert_ok_with_env(
        &test_home,
        "it-apikey-keystore",
        &[("ESDIAG_KEYSTORE_PASSWORD", keystore_password)],
        "collect apikey keystore",
    );
}

#[derive(Clone, Copy)]
enum SecurityMode {
    Enabled,
    Disabled,
}

struct RunningEsContainer {
    runtime: String,
    name: String,
}

impl RunningEsContainer {
    fn start(
        runtime: &str,
        image: &str,
        host_port: u16,
        security: SecurityMode,
        elastic_password: Option<&str>,
    ) -> Result<Self, String> {
        let name = format!("esdiag-it-{}", Uuid::new_v4().simple());
        let mut args = vec![
            "run".to_string(),
            "-d".to_string(),
            "--rm".to_string(),
            "--name".to_string(),
            name.clone(),
            "-p".to_string(),
            format!("{host_port}:9200"),
            "-e".to_string(),
            "discovery.type=single-node".to_string(),
            "-e".to_string(),
            "xpack.security.http.ssl.enabled=false".to_string(),
            "-e".to_string(),
            "xpack.ml.use_auto_machine_memory_percent=true".to_string(),
            "-e".to_string(),
            "_JAVA_OPTIONS=-XX:UseSVE=0".to_string(),
            "-e".to_string(),
            "ES_JAVA_OPTS=-Xms1g -Xmx1g".to_string(),
        ];
        match security {
            SecurityMode::Enabled => {
                args.push("-e".to_string());
                args.push("xpack.security.enabled=true".to_string());
                args.push("-e".to_string());
                args.push(format!("ELASTIC_PASSWORD={}", elastic_password.unwrap_or("changeme")));
            }
            SecurityMode::Disabled => {
                args.push("-e".to_string());
                args.push("xpack.security.enabled=false".to_string());
            }
        }
        args.push(image.to_string());

        let status = Command::new(runtime)
            .args(args)
            .status()
            .map_err(|e| format!("failed to start container: {e}"))?;
        if !status.success() {
            return Err(format!("container runtime returned failure: {status}"));
        }
        Ok(Self {
            runtime: runtime.to_string(),
            name,
        })
    }
}

impl Drop for RunningEsContainer {
    fn drop(&mut self) {
        let _ = Command::new(&self.runtime).args(["rm", "-f", &self.name]).status();
    }
}

fn run_collect_assert_ok(home: &tempfile::TempDir, host: &str, label: &str) {
    run_collect_assert_ok_with_env(home, host, &[], label)
}

fn run_collect_assert_ok_with_env(home: &tempfile::TempDir, host: &str, extra_env: &[(&str, &str)], label: &str) {
    let collect_out = tempdir().expect("collect output");
    let collect = run_esdiag(
        &[
            "collect",
            host,
            collect_out.path().to_str().expect("path"),
            "--type",
            "minimal",
        ],
        home,
        extra_env,
    );
    assert_success(&collect, label);
    assert_has_diagnostic_manifest(collect_out.path(), home.path(), label, &collect);
}

fn assert_has_diagnostic_manifest(output_root: &Path, home_root: &Path, label: &str, output: &Output) {
    if let Some(diag_dir) = find_diag_dir(output_root) {
        assert!(diag_dir.join("diagnostic_manifest.json").exists());
        return;
    }

    if let Some(zip_path) = find_diag_zip(output_root) {
        let file = fs::File::open(zip_path).expect("zip exists");
        let mut archive = ZipArchive::new(file).expect("zip is valid");
        assert!(archive.by_name("diagnostic_manifest.json").is_ok());
        return;
    }

    // Some environments can fall back to writing under ~/.esdiag/last_run.
    let fallback = home_root.join(".esdiag").join("last_run");
    if let Some(diag_dir) = find_diag_dir(&fallback) {
        assert!(diag_dir.join("diagnostic_manifest.json").exists());
        return;
    }
    if let Some(zip_path) = find_diag_zip(&fallback) {
        let file = fs::File::open(zip_path).expect("zip exists");
        let mut archive = ZipArchive::new(file).expect("zip is valid");
        assert!(archive.by_name("diagnostic_manifest.json").is_ok());
        return;
    }

    let out_ls = list_dir(output_root);
    let fallback_ls = list_dir(&fallback);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    panic!(
        "{label}: diagnostic output not found\noutput_root={}\noutput_root_entries={out_ls:?}\nfallback_entries={fallback_ls:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output_root.display(),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_collect_kibana_mock_workflow() {
    async fn status_handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "name": "mock-kibana",
            "version": { "number": "8.19.0" }
        }))
    }

    async fn spaces_handler() -> Json<serde_json::Value> {
        Json(serde_json::json!([
            { "id": "default" },
            { "id": "security" }
        ]))
    }

    async fn stats_handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "name": "mock-kibana",
            "status": "green"
        }))
    }

    async fn alerts_handler(
        AxumPath(space): AxumPath<String>,
        Query(params): Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        let page = params
            .get("page")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1)
            .max(1);
        let per_page = params
            .get("per_page")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(100);
        let total = 125;
        let start = (page - 1) * per_page;
        let end = total.min(start + per_page);
        let data: Vec<_> = (start..end)
            .map(|idx| serde_json::json!({ "id": format!("{space}-{idx}") }))
            .collect();

        Json(serde_json::json!({
            "page": page,
            "per_page": per_page,
            "total": total,
            "data": data
        }))
    }

    let app = Router::new()
        .route("/api/status", get(status_handler))
        .route("/api/spaces/space", get(spaces_handler))
        .route("/api/stats", get(stats_handler))
        .route("/s/{space}/api/alerts/_find", get(alerts_handler));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock kibana");
    let addr = listener.local_addr().expect("listener addr");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("serve mock kibana");
    });

    let home = tempdir().expect("temp home");
    let home_path = home.path().to_path_buf();
    let host_url = format!("http://{addr}");

    let host_args = vec![
        "host".to_string(),
        "add".to_string(),
        "mock-kibana".to_string(),
        "kibana".to_string(),
        host_url.clone(),
    ];
    let host_output = tokio::task::spawn_blocking({
        let home_path = home_path.clone();
        move || run_esdiag_with_home(host_args, home_path, vec![])
    })
    .await
    .expect("join host command");
    assert_success(&host_output, "configure mock kibana host");

    let out_dir = home.path().join("collect-out");
    fs::create_dir_all(&out_dir).expect("create output dir");
    let collect_args = vec![
        "collect".to_string(),
        "mock-kibana".to_string(),
        out_dir.to_str().expect("out dir").to_string(),
        "--type".to_string(),
        "minimal".to_string(),
        "--include".to_string(),
        "kibana_stats,kibana_alerts".to_string(),
    ];
    let collect_output = tokio::task::spawn_blocking({
        let home_path = home_path.clone();
        move || run_esdiag_with_home(collect_args, home_path, vec![])
    })
    .await
    .expect("join collect command");
    assert_success(&collect_output, "collect mock kibana workflow");

    let extracted = extract_diag_zip_to_temp(&out_dir).expect("extract collected archive");
    assert!(extracted.path.join("diagnostic_manifest.json").exists());
    assert!(extracted.path.join("kibana_status.json").exists());
    assert!(extracted.path.join("kibana_spaces.json").exists());
    assert!(extracted.path.join("kibana_stats.json").exists());
    assert!(
        extracted
            .path
            .join("spaces/default/pages/page-0001/kibana_alerts.json")
            .exists()
    );
    assert!(
        extracted
            .path
            .join("spaces/default/pages/page-0002/kibana_alerts.json")
            .exists()
    );
    assert!(
        extracted
            .path
            .join("spaces/security/pages/page-0001/kibana_alerts.json")
            .exists()
    );
    assert!(
        extracted
            .path
            .join("spaces/security/pages/page-0002/kibana_alerts.json")
            .exists()
    );

    let _ = shutdown_tx.send(());
    let _ = server.await;
}

fn run_esdiag(args: &[&str], home: &tempfile::TempDir, extra_env: &[(&str, &str)]) -> Output {
    run_esdiag_with_home(
        args.iter().map(|arg| (*arg).to_string()).collect(),
        home.path().to_path_buf(),
        extra_env
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
    )
}

fn run_esdiag_with_home(args: Vec<String>, home_path: PathBuf, extra_env: Vec<(String, String)>) -> Output {
    let home_path = home_path.to_str().expect("home path");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_esdiag"));
    cmd.args(args)
        .env("HOME", home_path)
        .env("USERPROFILE", home_path)
        .env("LOG_LEVEL", "debug");
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.output().expect("run esdiag")
}

fn run_kibana_collect_matrix_case(host_env: &str) {
    let host = env::var(host_env).unwrap_or_else(|_| {
        panic!(
            "Set {} to a known Kibana host before running this ignored test",
            host_env
        )
    });
    let dir = tempdir().expect("temp dir");
    let out_dir = dir.path().to_str().expect("output dir");

    let status = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args([
            "collect",
            host.as_str(),
            out_dir,
            "--type",
            "minimal",
            "--include",
            "kibana_stats",
        ])
        .status()
        .expect("Failed to execute Kibana collect process");

    assert!(status.success());

    let extracted = extract_diag_zip_to_temp(dir.path()).expect("Should have generated a zip");
    assert!(extracted.path.join("diagnostic_manifest.json").exists());
    assert!(extracted.path.join("kibana_status.json").exists());
    assert!(extracted.path.join("kibana_spaces.json").exists());
    assert!(extracted.path.join("kibana_stats.json").exists());
}

#[test]
#[ignore = "requires external localhost Kibana service"]
fn test_collect_kibana_localhost_no_auth() {
    let home = tempdir().expect("temp home");
    let host_output = run_esdiag(
        &["host", "add", "localhost-kibana", "kibana", "http://localhost:5601"],
        &home,
        &[],
    );
    assert_success(&host_output, "configure localhost kibana host");

    let out_dir = home.path().join("collect-out");
    fs::create_dir_all(&out_dir).expect("create output dir");
    let collect_output = run_esdiag(
        &[
            "collect",
            "localhost-kibana",
            out_dir.to_str().expect("out dir"),
            "--type",
            "minimal",
            "--include",
            "kibana_stats",
        ],
        &home,
        &[],
    );
    assert_success(&collect_output, "collect localhost kibana");

    let extracted = extract_diag_zip_to_temp(&out_dir).expect("extract collected archive");
    assert!(extracted.path.join("diagnostic_manifest.json").exists());
    assert!(extracted.path.join("kibana_status.json").exists());
    assert!(extracted.path.join("kibana_spaces.json").exists());
    assert!(extracted.path.join("kibana_stats.json").exists());
}

#[test]
#[ignore = "requires external localhost Kibana service"]
fn test_kibana_localhost_accepts_kibana_pagination_query_shapes() {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("http client");

    let cases = [
        (
            "pageSize style",
            "http://localhost:5601/api/endpoint/metadata?page=1&pageSize=2",
        ),
        (
            "per_page style",
            "http://localhost:5601/api/detection_engine/rules/_find?sort_field=enabled&sort_order=asc&page=1&per_page=2",
        ),
    ];

    for (label, url) in cases {
        let response = client
            .get(url)
            .header("kbn-xsrf", "true")
            .send()
            .unwrap_or_else(|err| panic!("{label}: request failed: {err}"));
        let status = response.status();
        let body = response.text().expect("response body");

        assert!(
            !(status.as_u16() == 400
                && (body.contains("query.pageIndex")
                    || body.contains("query.page")
                    || body.contains("definition for this key is missing"))),
            "{label}: pagination query shape was rejected\nstatus={status}\nbody={body}"
        );
    }
}

#[test]
#[ignore = "requires external Kibana 6.8.x service"]
fn test_collect_kibana_6_8_x_compatibility() {
    run_kibana_collect_matrix_case("ESDIAG_TEST_KIBANA_6_8_HOST");
}

#[test]
#[ignore = "requires external Kibana 7.17.x service"]
fn test_collect_kibana_7_17_x_compatibility() {
    run_kibana_collect_matrix_case("ESDIAG_TEST_KIBANA_7_17_HOST");
}

#[test]
#[ignore = "requires external Kibana 8.19.x service"]
fn test_collect_kibana_8_19_x_compatibility() {
    run_kibana_collect_matrix_case("ESDIAG_TEST_KIBANA_8_19_HOST");
}

#[test]
#[ignore = "requires external Kibana 9.x service"]
fn test_collect_kibana_9_x_compatibility() {
    run_kibana_collect_matrix_case("ESDIAG_TEST_KIBANA_9_HOST");
}

fn assert_success(output: &Output, label: &str) {
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("{label} failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
    }
}

fn list_dir(path: &Path) -> Vec<String> {
    match fs::read_dir(path) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn create_api_key(port: u16, elastic_password: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let body = serde_json::json!({ "name": "esdiag-it" });
    let resp = client
        .post(format!("http://127.0.0.1:{port}/_security/api_key"))
        .basic_auth("elastic", Some(elastic_password))
        .json(&body)
        .send()
        .map_err(|e| format!("api key request: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("api key status {status}: {text}"));
    }
    let json: Value = resp.json().map_err(|e| format!("api key response parse: {e}"))?;
    json.get("encoded")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("missing encoded api key in response: {json}"))
}

fn wait_for_es(port: u16, security: SecurityMode, elastic_password: Option<&str>) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(180);

    while Instant::now() < deadline {
        let mut req = client.get(format!("http://127.0.0.1:{port}"));
        if let SecurityMode::Enabled = security {
            req = req.basic_auth("elastic", elastic_password);
        }
        match req.send() {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(_) | Err(_) => sleep(Duration::from_secs(2)),
        }
    }
    Err(format!("timed out waiting for Elasticsearch on port {port}"))
}

fn container_runtime() -> Option<String> {
    for runtime in ["podman", "docker"] {
        if Command::new(runtime).arg("--version").output().is_ok() {
            return Some(runtime.to_string());
        }
    }
    None
}

fn elastic_image() -> String {
    let registry = env::var("ELASTIC_CONTAINER_REGISTRY").unwrap_or_else(|_| "docker.elastic.co".to_string());
    let version = env::var("ELASTIC_VERSION").unwrap_or_else(|_| "9.3.0".to_string());
    format!("{registry}/elasticsearch/elasticsearch:{version}")
}
