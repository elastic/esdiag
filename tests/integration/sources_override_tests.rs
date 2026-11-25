use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tokio::net::TcpSocket;

use tempfile::tempdir;

/// Human-readable marker of a sources.yml product-validation failure. The reject tests prove it
/// appears on that failure, so its absence in the accept tests is meaningful.
const SOURCES_VALIDATION_ERROR: &str = "valid logstash sources.yml";

/// Override file name, passed relative to the child's working directory. It holds the words of
/// several failure categories, which must not affect the reported category.
const OVERRIDE_FILE: &str = "auth-collect-process-send-secret.yml";

const MISMATCHED_OVERRIDE: &str = r#"version:
  versions:
    "> 5.0.0": "/"
"#;

const LOGSTASH_OVERRIDE: &str = r#"logstash_node:
  versions:
    "> 5.0.0": "/_node"
logstash_node_stats:
  versions:
    "> 5.0.0": "/_node/stats"
logstash_plugins:
  versions:
    "> 5.0.0": "/_node/plugins"
logstash_version:
  versions:
    "> 5.0.0": "/"
"#;

#[test]
fn test_collect_sources_override_rejects_wrong_product_for_logstash_host() {
    let output = run_collect_with_override(MISMATCHED_OVERRIDE);

    assert!(
        !output.status.success(),
        "collect should fail for mismatched override file"
    );
    let failure = parse_failure(&output);
    assert_eq!(failure["category"], "invalid_input", "{failure}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(SOURCES_VALIDATION_ERROR), "stderr: {stderr}");
}

#[test]
fn test_collect_sources_override_accepts_matching_product_before_connection() {
    let output = run_collect_with_override(LOGSTASH_OVERRIDE);

    assert!(
        !output.status.success(),
        "collect should still fail without a live Logstash endpoint"
    );
    // Passing validation means the command reached the (closed) endpoint and failed collecting.
    let failure = parse_failure(&output);
    assert_eq!(failure["category"], "collection_failed", "{failure}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains(SOURCES_VALIDATION_ERROR),
        "matching override should not fail product validation: {stderr}"
    );
}

#[test]
fn test_process_sources_override_rejects_wrong_product_for_manifest() {
    let output = run_process_with_override(MISMATCHED_OVERRIDE);

    assert!(
        !output.status.success(),
        "process should fail for mismatched override file"
    );
    let failure = parse_failure(&output);
    assert_eq!(failure["category"], "invalid_input", "{failure}");
    assert!(failure.get("failed_stage").is_none(), "{failure}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(SOURCES_VALIDATION_ERROR), "stderr: {stderr}");
}

#[test]
fn test_process_sources_override_accepts_matching_manifest_product() {
    let output = run_process_with_override(LOGSTASH_OVERRIDE);

    assert!(
        !output.status.success(),
        "process should still fail because required Logstash files are absent"
    );
    // Passing validation means the job started and failed inside the process stage.
    let failure = parse_failure(&output);
    assert_eq!(failure["category"], "processing_failed", "{failure}");
    assert_eq!(failure["failed_stage"], "process", "{failure}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains(SOURCES_VALIDATION_ERROR),
        "matching override should not fail product validation: {stderr}"
    );
}

fn run_collect_with_override(override_yaml: &str) -> Output {
    let home = tempdir().expect("temp home");
    // Held until the command finishes so no other process can take the port.
    let (_reserved, port) = closed_local_port();
    let hosts_path = home.path().join("hosts.yml");
    fs::write(
        &hosts_path,
        format!(
            "logstash-local:\n  auth: NoAuth\n  app: logstash\n  roles:\n    - collect\n  url: http://127.0.0.1:{}\n",
            port
        ),
    )
    .expect("write hosts");

    let override_dir = tempdir().expect("override dir");
    fs::write(override_dir.path().join(OVERRIDE_FILE), override_yaml).expect("write override");

    let output_dir = tempdir().expect("output dir");
    run_esdiag(
        &[
            "--format",
            "json",
            "collect",
            "logstash-local",
            output_dir.path().to_str().expect("out path"),
            "--type",
            "minimal",
            "--sources",
            OVERRIDE_FILE,
        ],
        home.path(),
        override_dir.path(),
        Some(&hosts_path),
    )
}

fn run_process_with_override(override_yaml: &str) -> Output {
    let home = tempdir().expect("temp home");
    let input_dir = tempdir().expect("input dir");
    fs::write(
        input_dir.path().join("diagnostic_manifest.json"),
        r#"{
  "product": "logstash",
  "timestamp": "2026-03-10T00:00:00Z"
}"#,
    )
    .expect("write manifest");

    let override_dir = tempdir().expect("override dir");
    fs::write(override_dir.path().join(OVERRIDE_FILE), override_yaml).expect("write override");

    // A directory output (not `-`) keeps stdout free for the structured outcome.
    let output_dir = tempdir().expect("output dir");
    run_esdiag(
        &[
            "--format",
            "json",
            "process",
            input_dir.path().to_str().expect("input path"),
            output_dir.path().to_str().expect("output path"),
            "--sources",
            OVERRIDE_FILE,
        ],
        home.path(),
        override_dir.path(),
        None,
    )
}

/// A loopback port that refuses connections: bound but never listening, so the
/// port stays reserved for as long as the returned socket is alive.
fn closed_local_port() -> (TcpSocket, u16) {
    let socket = TcpSocket::new_v4().expect("create socket");
    socket
        .bind("127.0.0.1:0".parse().expect("loopback address"))
        .expect("bind ephemeral port");
    let port = socket.local_addr().expect("ephemeral port address").port();
    (socket, port)
}

fn parse_failure(output: &Output) -> serde_json::Value {
    let failure: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "parse JSON failure outcome: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(failure["result"], "command_failed", "{failure}");
    failure
}

/// Runs from `cwd` so `--sources` can be a relative path.
fn run_esdiag(args: &[&str], home: &Path, cwd: &Path, hosts_path: Option<&Path>) -> Output {
    let home_path = home.to_str().expect("home path");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_esdiag"));
    cmd.args(args)
        .current_dir(cwd)
        .env("HOME", home_path)
        .env("USERPROFILE", home_path)
        .env("LOG_LEVEL", "debug")
        .env_remove("CLAUDECODE");

    if let Some(hosts_path) = hosts_path {
        cmd.env("ESDIAG_HOSTS", hosts_path);
    }

    cmd.output().expect("run esdiag")
}
