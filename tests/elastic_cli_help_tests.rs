// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use std::process::Command;

#[test]
fn elastic_cli_invocation_without_subcommand_prints_extension_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .env("ESDIAG_ELASTIC_CLI", "1")
        .output()
        .expect("run esdiag");

    assert!(
        output.status.success(),
        "expected success, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Elastic CLI extension examples:"));
    assert!(stdout.contains("elastic diag collect .es ./out"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("No subcommand provided"));
}

#[test]
fn elastic_cli_help_keeps_extension_block_separated() {
    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .arg("--help")
        .env("ESDIAG_ELASTIC_CLI", "1")
        .output()
        .expect("run esdiag");

    assert!(
        output.status.success(),
        "expected success, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\n\nElastic CLI extension commands:"));
}

#[test]
fn bare_extension_process_requires_an_application_target() {
    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .arg("process")
        .env("ESDIAG_ELASTIC_CLI", "1")
        .output()
        .expect("run esdiag");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("requires an explicit application-bearing input"));
    assert!(stderr.contains(".es"));
    assert!(stderr.contains(".kb"));
}

#[test]
fn extension_help_subcommand_uses_elastic_diag_name() {
    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["help", "process"])
        .env("ESDIAG_ELASTIC_CLI", "1")
        .output()
        .expect("run process help");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: elastic diag process"));
}

#[test]
fn extension_help_rejects_standalone_commands() {
    for arguments in [
        ["help", "host"],
        ["host", "--help"],
        ["help", "secret"],
        ["secret", "--help"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
            .args(arguments)
            .env("ESDIAG_ELASTIC_CLI", "1")
            .output()
            .expect("run extension help");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("standalone 'esdiag'"));
        assert!(!stderr.contains("Usage: elastic diag host"));
    }
}

#[test]
fn standalone_help_excludes_extension_guidance_and_upload_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .arg("--help")
        .env_remove("ESDIAG_ELASTIC_CLI")
        .output()
        .expect("run standalone help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: esdiag"));
    assert!(stdout.contains("  send "));
    assert!(!stdout.contains("Elastic CLI extension commands:"));
    assert!(!stdout.contains("  upload "));
}

#[test]
fn elastic_diag_executable_name_selects_the_restricted_profile() {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let executable_name = if cfg!(windows) {
        "elastic-diag.exe"
    } else {
        "elastic-diag"
    };
    let executable = tmp.path().join(executable_name);
    std::fs::copy(env!("CARGO_BIN_EXE_esdiag"), &executable).expect("copy binary");

    let help = Command::new(&executable).output().expect("run elastic-diag");
    assert!(
        help.status.success(),
        "help failed: {}",
        String::from_utf8_lossy(&help.stderr)
    );
    let stdout = String::from_utf8_lossy(&help.stdout);
    assert!(stdout.contains("Usage: elastic diag"));
    assert!(stdout.contains("  send "));
    assert!(stdout.contains("  output "));
    assert!(!stdout.contains("  upload "));
    assert!(!stdout.contains("  host "));
    assert!(!stdout.contains("  keystore "));

    let rejected = Command::new(&executable)
        .args(["host", "list"])
        .output()
        .expect("run unsupported command");
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("not part of the 'elastic diag' profile"));
}
