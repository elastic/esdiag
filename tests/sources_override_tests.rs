use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::tempdir;

#[test]
fn test_collect_sources_override_rejects_wrong_product_for_logstash_host() {
    let home = tempdir().expect("temp home");
    let hosts_path = home.path().join("hosts.yml");
    fs::write(
        &hosts_path,
        r#"logstash-local:
  auth: NoAuth
  app: logstash
  roles:
    - collect
  url: http://127.0.0.1:9600
"#,
    )
    .expect("write hosts");

    let override_dir = tempdir().expect("override dir");
    let override_path = override_dir.path().join("sources.yml");
    fs::write(
        &override_path,
        r#"version:
  versions:
    "> 5.0.0": "/"
"#,
    )
    .expect("write override");

    let output_dir = tempdir().expect("output dir");
    let output = run_esdiag(
        &[
            "collect",
            "logstash-local",
            output_dir.path().to_str().expect("out path"),
            "--type",
            "minimal",
            "--sources",
            override_path.to_str().expect("override path"),
        ],
        home.path(),
        Some(&hosts_path),
    );

    assert!(
        !output.status.success(),
        "collect should fail for mismatched override file"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("valid logstash sources.yml"));
}

#[test]
fn test_collect_sources_override_accepts_matching_product_before_connection() {
    let home = tempdir().expect("temp home");
    let hosts_path = home.path().join("hosts.yml");
    fs::write(
        &hosts_path,
        r#"logstash-local:
  auth: NoAuth
  app: logstash
  roles:
    - collect
  url: http://127.0.0.1:9600
"#,
    )
    .expect("write hosts");

    let override_dir = tempdir().expect("override dir");
    let override_path = override_dir.path().join("sources.yml");
    fs::write(
        &override_path,
        r#"logstash_node:
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
"#,
    )
    .expect("write override");

    let output_dir = tempdir().expect("output dir");
    let output = run_esdiag(
        &[
            "collect",
            "logstash-local",
            output_dir.path().to_str().expect("out path"),
            "--type",
            "minimal",
            "--sources",
            override_path.to_str().expect("override path"),
        ],
        home.path(),
        Some(&hosts_path),
    );

    assert!(
        !output.status.success(),
        "collect should still fail without a live Logstash endpoint"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("valid logstash sources.yml"),
        "matching override should not fail product validation: {stderr}"
    );
}

#[test]
fn test_process_sources_override_rejects_wrong_product_for_manifest() {
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
    let override_path = override_dir.path().join("sources.yml");
    fs::write(
        &override_path,
        r#"version:
  versions:
    "> 5.0.0": "/"
"#,
    )
    .expect("write override");

    let output = run_esdiag(
        &[
            "process",
            input_dir.path().to_str().expect("input path"),
            "-",
            "--sources",
            override_path.to_str().expect("override path"),
        ],
        home.path(),
        None,
    );

    assert!(
        !output.status.success(),
        "process should fail for mismatched override file"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("valid logstash sources.yml"));
}

#[test]
fn test_process_sources_override_accepts_matching_manifest_product() {
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
    let override_path = override_dir.path().join("sources.yml");
    fs::write(
        &override_path,
        r#"logstash_node:
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
"#,
    )
    .expect("write override");

    let output = run_esdiag(
        &[
            "process",
            input_dir.path().to_str().expect("input path"),
            "-",
            "--sources",
            override_path.to_str().expect("override path"),
        ],
        home.path(),
        None,
    );

    assert!(
        !output.status.success(),
        "process should still fail because required Logstash files are absent"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("valid logstash sources.yml"),
        "matching override should not fail product validation: {stderr}"
    );
}

fn run_esdiag(args: &[&str], home: &Path, hosts_path: Option<&Path>) -> Output {
    let home_path = home.to_str().expect("home path");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_esdiag"));
    cmd.args(args)
        .env("HOME", home_path)
        .env("USERPROFILE", home_path)
        .env("LOG_LEVEL", "debug");

    if let Some(hosts_path) = hosts_path {
        cmd.env("ESDIAG_HOSTS", hosts_path);
    }

    cmd.output().expect("run esdiag")
}
