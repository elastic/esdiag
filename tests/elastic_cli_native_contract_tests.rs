// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

#[cfg(unix)]
#[test]
#[ignore = "requires Elastic CLI v0.3.0 with this checkout registered as the diag extension"]
fn native_elastic_cli_forwards_arguments_and_context() {
    use std::{env, fs, os::unix::fs::PermissionsExt, process::Command};

    let version = Command::new("elastic")
        .arg("version")
        .output()
        .expect("Elastic CLI must be installed");
    assert!(
        String::from_utf8_lossy(&version.stdout).contains("Elastic CLI v0.3.0"),
        "contract test requires Elastic CLI v0.3.0"
    );

    let tmp = tempfile::TempDir::new().expect("temp dir");
    let shim = tmp.path().join("esdiag");
    fs::copy(env!("CARGO_BIN_EXE_esdiag"), &shim).expect("copy current esdiag binary");

    let config = tmp.path().join(".elasticrc.yml");
    fs::write(
        &config,
        "current_context: contract\ncontexts:\n  contract:\n    elasticsearch:\n      url: https://contract.example:9200\n      auth:\n        api_key: contract-secret\n",
    )
    .expect("write config");

    let mut path = vec![tmp.path().to_path_buf()];
    path.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    let path = env::join_paths(path).expect("join PATH");
    let help = Command::new("elastic")
        .args(["diag", "help", "process"])
        .env("PATH", &path)
        .env("ELASTIC_CLI_CONFIG_FILE", &config)
        .output()
        .expect("run delegated help");
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage: elastic diag process"));
    let unsupported_help = Command::new("elastic")
        .args(["diag", "help", "host"])
        .env("PATH", &path)
        .env("ELASTIC_CLI_CONFIG_FILE", &config)
        .output()
        .expect("run unsupported delegated help");
    assert!(!unsupported_help.status.success());
    assert!(!String::from_utf8_lossy(&unsupported_help.stdout).contains("Usage: elastic diag host"));

    fs::write(
        &shim,
        r#"#!/bin/sh
printf 'args=%s\n' "$*"
printf 'marker=%s\n' "$ESDIAG_ELASTIC_CLI"
test -n "$ELASTIC_ES_URL" && printf 'es_url=set\n'
test -n "$ELASTIC_ES_API_KEY" && printf 'es_api_key=set\n'
"#,
    )
    .expect("write shim");
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).expect("make shim executable");

    let output = Command::new("elastic")
        .args(["diag", "process", ".es"])
        .env("PATH", path)
        .env("ELASTIC_CLI_CONFIG_FILE", config)
        .output()
        .expect("run Elastic CLI extension");

    assert!(
        output.status.success(),
        "contract failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("args=process .es"));
    assert!(stdout.contains("marker=1"));
    assert!(stdout.contains("es_url=set"));
    assert!(stdout.contains("es_api_key=set"));
    assert!(!stdout.contains("contract-secret"));
}
