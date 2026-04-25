//! Ignored external Logstash collection tests.
//!
//! These tests validate Logstash support collection against real external
//! services. They are intentionally ignored because they depend on
//! environment-specific infrastructure.
//!
//! Set `*_URL` for each version you want to exercise:
//!
//! - `ESDIAG_LOGSTASH_68_URL`
//! - `ESDIAG_LOGSTASH_717_URL`
//! - `ESDIAG_LOGSTASH_819_URL`
//! - `ESDIAG_LOGSTASH_9_URL`
//!
//! Authentication is optional and can be provided per target with either
//! `*_APIKEY` or `*_USERNAME` and `*_PASSWORD`.
//!
//! If the target uses a self-signed or otherwise invalid TLS certificate, set
//! `*_ACCEPT_INVALID_CERTS=true`.
//!
//! Example:
//!
//! ```sh
//! ESDIAG_LOGSTASH_68_URL=https://ls68.example.org:9600 \
//! ESDIAG_LOGSTASH_68_USERNAME=elastic \
//! ESDIAG_LOGSTASH_68_PASSWORD=changeme \
//! ESDIAG_LOGSTASH_68_ACCEPT_INVALID_CERTS=true \
//! cargo test --test logstash_collection_tests -- --ignored
//! ```

use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;
use zip::ZipArchive;

struct ExtractedDiag {
    _tmp: tempfile::TempDir,
    path: PathBuf,
}

struct ExternalLogstashConfig {
    host_name: String,
    url: String,
    apikey: Option<String>,
    username: Option<String>,
    password: Option<String>,
    accept_invalid_certs: bool,
}

impl ExternalLogstashConfig {
    fn from_env(prefix: &str) -> Option<Self> {
        let url = env::var(format!("{prefix}_URL")).ok()?;
        let host_name = format!("{}-host", prefix.to_lowercase());
        let apikey = env::var(format!("{prefix}_APIKEY")).ok();
        let username = env::var(format!("{prefix}_USERNAME")).ok();
        let password = env::var(format!("{prefix}_PASSWORD")).ok();
        let accept_invalid_certs = env::var(format!("{prefix}_ACCEPT_INVALID_CERTS"))
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);
        Some(Self {
            host_name,
            url,
            apikey,
            username,
            password,
            accept_invalid_certs,
        })
    }
}

#[test]
#[ignore = "requires an externally managed Logstash 6.8.x instance"]
fn test_collect_logstash_68_support() {
    run_external_logstash_collect_test(
        "ESDIAG_LOGSTASH_68",
        false,
        &[
            "diagnostic_manifest.json",
            "logstash_node.json",
            "logstash_node_stats.json",
            "logstash_nodes_hot_threads.json",
            "logstash_nodes_hot_threads_human.txt",
            "logstash_plugins.json",
            "logstash_version.json",
        ],
        &["logstash_health_report.json"],
    );
}

#[test]
#[ignore = "requires an externally managed Logstash 7.17.x instance"]
fn test_collect_logstash_717_support() {
    run_external_logstash_collect_test(
        "ESDIAG_LOGSTASH_717",
        false,
        &[
            "diagnostic_manifest.json",
            "logstash_node.json",
            "logstash_node_stats.json",
            "logstash_nodes_hot_threads.json",
            "logstash_nodes_hot_threads_human.txt",
            "logstash_plugins.json",
            "logstash_version.json",
        ],
        &["logstash_health_report.json"],
    );
}

#[test]
#[ignore = "requires an externally managed Logstash 8.19.x instance"]
fn test_collect_logstash_819_support() {
    run_external_logstash_collect_test(
        "ESDIAG_LOGSTASH_819",
        true,
        &[
            "diagnostic_manifest.json",
            "logstash_node.json",
            "logstash_node_stats.json",
            "logstash_nodes_hot_threads.json",
            "logstash_nodes_hot_threads_human.txt",
            "logstash_plugins.json",
            "logstash_version.json",
            "logstash_health_report.json",
        ],
        &[],
    );
}

#[test]
#[ignore = "requires an externally managed Logstash 9.x instance"]
fn test_collect_logstash_9_support() {
    run_external_logstash_collect_test(
        "ESDIAG_LOGSTASH_9",
        true,
        &[
            "diagnostic_manifest.json",
            "logstash_node.json",
            "logstash_node_stats.json",
            "logstash_nodes_hot_threads.json",
            "logstash_nodes_hot_threads_human.txt",
            "logstash_plugins.json",
            "logstash_version.json",
            "logstash_health_report.json",
        ],
        &[],
    );
}

fn run_external_logstash_collect_test(
    env_prefix: &str,
    expect_health_report: bool,
    expected_files: &[&str],
    unexpected_files: &[&str],
) {
    let Some(config) = ExternalLogstashConfig::from_env(env_prefix) else {
        eprintln!("Skipping {env_prefix}: missing {env_prefix}_URL");
        return;
    };

    let test_home = tempdir().expect("temp home");

    let mut host_args = vec![
        "host".to_string(),
        config.host_name.clone(),
        "logstash".to_string(),
        config.url.clone(),
    ];

    if let Some(apikey) = &config.apikey {
        host_args.push("--apikey".to_string());
        host_args.push(apikey.clone());
    } else if let (Some(username), Some(password)) = (&config.username, &config.password) {
        host_args.push("--user".to_string());
        host_args.push(username.clone());
        host_args.push("--password".to_string());
        host_args.push(password.clone());
    }

    if config.accept_invalid_certs {
        host_args.push("--accept-invalid-certs".to_string());
        host_args.push("true".to_string());
    }

    let host_arg_refs: Vec<&str> = host_args.iter().map(String::as_str).collect();
    let host = run_esdiag(&host_arg_refs, &test_home);
    assert_success(&host, &format!("{env_prefix} host setup"));

    let collect_out = tempdir().expect("collect output");
    let collect = run_esdiag(
        &[
            "collect",
            &config.host_name,
            collect_out.path().to_str().expect("collect path"),
            "--type",
            "support",
        ],
        &test_home,
    );
    assert_success(&collect, &format!("{env_prefix} collect support"));

    let extracted = extract_diag_zip_to_temp(collect_out.path()).expect("should have a zip output");
    for path in expected_files {
        assert!(
            extracted.path.join(path).exists(),
            "{env_prefix} expected file {path} to exist"
        );
    }
    for path in unexpected_files {
        assert!(
            !extracted.path.join(path).exists(),
            "{env_prefix} expected file {path} to be absent"
        );
    }

    let manifest_content = fs::read_to_string(extracted.path.join("diagnostic_manifest.json")).expect("manifest");
    let manifest: Value = serde_json::from_str(&manifest_content).expect("manifest json");
    assert_eq!(manifest["product"].as_str(), Some("logstash"));
    assert_eq!(manifest["mode"].as_str(), Some("support"));

    let apis = manifest["collected_apis"].as_array().expect("collected_apis array");
    let api_names: Vec<&str> = apis.iter().filter_map(|v| v.as_str()).collect();
    for required in [
        "logstash_node",
        "logstash_node_stats",
        "logstash_plugins",
        "logstash_version",
        "logstash_nodes_hot_threads",
        "logstash_nodes_hot_threads_human",
    ] {
        assert!(
            api_names.contains(&required),
            "{env_prefix} missing collected api {required}"
        );
    }
    assert_eq!(
        api_names.contains(&"logstash_health_report"),
        expect_health_report,
        "{env_prefix} health report expectation mismatch"
    );
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

fn find_diag_zip(path: &Path) -> Option<PathBuf> {
    for entry in fs::read_dir(path).ok()? {
        let entry = entry.ok()?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let valid_prefix = file_name.starts_with("logstash-api-diagnostics-");
        if entry.file_type().ok()?.is_file() && valid_prefix && file_name.ends_with(".zip") {
            return Some(entry.path());
        }
    }
    None
}

fn run_esdiag(args: &[&str], home: &tempfile::TempDir) -> Output {
    let home_path = home.path().to_str().expect("home path");
    Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(args)
        .env("HOME", home_path)
        .env("USERPROFILE", home_path)
        .env("LOG_LEVEL", "debug")
        .output()
        .expect("run esdiag")
}

fn assert_success(output: &Output, label: &str) {
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("{label} failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
    }
}
