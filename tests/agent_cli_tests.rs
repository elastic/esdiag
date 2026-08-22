use std::{
    io::BufRead,
    path::Path,
    process::{Command, Output, Stdio},
    sync::mpsc,
    time::Duration,
};
use tempfile::TempDir;

fn setup_home() -> TempDir {
    let home = TempDir::new().expect("temp dir");
    std::fs::create_dir_all(home.path().join(".esdiag")).expect("create config dir");
    home
}

fn run_esdiag(args: &[&str], home: &TempDir, extra_env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_esdiag"));
    cmd.args(args)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("ESDIAG_HOSTS", home.path().join(".esdiag").join("hosts.yml"))
        .env("ESDIAG_KEYSTORE", home.path().join(".esdiag").join("secrets.yml"))
        .env("LOG_LEVEL", "debug");

    for (key, value) in extra_env {
        cmd.env(key, value);
    }

    cmd.output().expect("run esdiag")
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed\nstatus: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_keystore_status_outcome(stdout: &str) {
    let outcome: serde_json::Value = yaml_serde::from_str(stdout).expect("parse YAML outcome");
    assert_eq!(outcome["result"], "keystore_status");
    assert_eq!(outcome["exists"], false);
    assert_eq!(outcome["unlock_active"], false);
}

#[test]
fn agent_flag_emits_yaml_outcome_without_info_logs() {
    let home = setup_home();
    let output = run_esdiag(&["--agent", "keystore", "status"], &home, &[]);
    assert_success(&output, "keystore status with --agent");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_keystore_status_outcome(&stdout);
    assert!(
        !stderr.contains("CLI unlock: inactive"),
        "agent mode should suppress info logs, stderr was:\n{stderr}"
    );
}

#[test]
fn claudecode_auto_enables_agent_mode() {
    let home = setup_home();
    let output = run_esdiag(&["keystore", "status"], &home, &[("CLAUDECODE", "1")]);
    assert_success(&output, "keystore status with CLAUDECODE");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_keystore_status_outcome(&stdout);
    assert!(
        !stderr.contains("CLI unlock: inactive"),
        "CLAUDECODE should enable warn-level suppression, stderr was:\n{stderr}"
    );
}

#[test]
fn claudecode_honors_explicit_json_format() {
    let home = setup_home();
    let output = run_esdiag(
        &["--format", "json", "keystore", "status"],
        &home,
        &[("CLAUDECODE", "1")],
    );
    assert_success(&output, "JSON keystore status with CLAUDECODE");

    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse JSON outcome");
    assert_eq!(outcome["result"], "keystore_status");
    assert_eq!(outcome["unlock_active"], false);
}

#[test]
fn debug_overrides_agent_warn_level() {
    let home = setup_home();
    let output = run_esdiag(&["--agent", "--debug", "keystore", "status"], &home, &[]);
    assert_success(&output, "keystore status with --agent --debug");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_keystore_status_outcome(&stdout);
}

#[test]
fn normal_mode_emits_yaml_outcome_when_logs_are_warn() {
    let home = setup_home();
    let output = run_esdiag(&["keystore", "status"], &home, &[("LOG_LEVEL", "warn")]);
    assert_success(&output, "keystore status with warn log level");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_keystore_status_outcome(&stdout);
    assert!(
        !stderr.contains("CLI unlock: inactive"),
        "warn log level should suppress info logs, stderr was:\n{stderr}"
    );
}

#[test]
fn json_format_emits_one_parseable_success_value() {
    let home = setup_home();
    let output = run_esdiag(&["--format", "json", "keystore", "status"], &home, &[]);
    assert_success(&output, "JSON keystore status");

    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse JSON outcome");
    assert_eq!(outcome["result"], "keystore_status");
    assert_eq!(outcome["unlock_active"], false);
}

#[test]
fn finite_failures_emit_one_parseable_yaml_value() {
    let home = setup_home();
    let output = run_esdiag(&["job", "run", "missing-job"], &home, &[]);
    assert!(!output.status.success(), "missing job unexpectedly succeeded");

    let outcome: serde_json::Value = yaml_serde::from_slice(&output.stdout).expect("parse YAML failure");
    assert_eq!(outcome["result"], "command_failed");
    assert_eq!(outcome["category"], "not_found");
    assert_eq!(outcome["resource"], "missing-job");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("\nError:") && !stderr.contains("\nLocation:"),
        "the runtime must not append an error report after a structured failure:\n{stderr}"
    );
}

#[cfg(feature = "agent")]
#[test]
fn process_ask_rejects_stdout_document_streams_before_processing() {
    let home = setup_home();
    let output = run_esdiag(
        &["process", "missing-input.zip", "-", "--ask", "What changed?"],
        &home,
        &[],
    );

    assert!(
        !output.status.success(),
        "incompatible process invocation unexpectedly succeeded"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("--ask cannot be used when process output is '-' because processed documents own stdout"),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn list_command_families_support_yaml_and_json_empty_collections() {
    for (command, result_key, collection_key) in [
        (vec!["host", "list"], "hosts_listed", "hosts"),
        (vec!["job", "list"], "jobs_listed", "jobs"),
    ] {
        for format in ["yaml", "json"] {
            let home = setup_home();
            let mut args = vec!["--format", format];
            args.extend(command.iter().copied());
            let output = run_esdiag(&args, &home, &[]);
            assert_success(&output, "empty list command");

            let value: serde_json::Value = if format == "yaml" {
                yaml_serde::from_slice(&output.stdout).expect("parse YAML outcome")
            } else {
                serde_json::from_slice(&output.stdout).expect("parse JSON outcome")
            };
            assert_eq!(value["result"], result_key);
            assert_eq!(value[collection_key], serde_json::json!([]));
        }
    }
}

#[test]
fn failures_honor_the_selected_json_format() {
    let home = setup_home();
    let output = run_esdiag(&["--format", "json", "job", "run", "missing-job"], &home, &[]);
    assert!(!output.status.success(), "missing job unexpectedly succeeded");

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse JSON failure");
    assert_eq!(value["result"], "command_failed");
    assert_eq!(value["category"], "not_found");
    assert_eq!(value["resource"], "missing-job");
}

#[cfg(feature = "keystore")]
#[test]
fn saved_job_stdout_export_remains_an_ndjson_stream() {
    let home = setup_home();
    let archive = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/archives/elasticsearch-api-diagnostics-6.8.23.zip");
    let archive = archive.to_str().expect("fixture path");
    std::fs::write(
        home.path().join(".esdiag").join("jobs.yml"),
        format!(
            "schema_version: 2\njobs:\n  stdout-job:\n    input:\n      type: load\n      uri: {archive}\n    process:\n      export:\n        type: stdout\n"
        ),
    )
    .expect("write saved stdout job");

    let output = run_esdiag(&["job", "run", "stdout-job"], &home, &[]);
    assert_success(&output, "run saved stdout process job");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let documents: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("parse NDJSON document: {error}\nstdout:\n{stdout}"))
        })
        .collect();
    assert!(!documents.is_empty(), "saved job did not emit any documents");
}

#[cfg(feature = "server")]
#[test]
fn serve_emits_readiness_outcome_before_waiting_for_shutdown() {
    let home = setup_home();
    let mut command = Command::new(env!("CARGO_BIN_EXE_esdiag"));
    command
        .args([
            "--format",
            "json",
            "serve",
            "--port",
            "0",
            home.path().to_str().expect("temporary path"),
        ])
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("ESDIAG_HOSTS", home.path().join(".esdiag").join("hosts.yml"))
        .env("ESDIAG_KEYSTORE", home.path().join(".esdiag").join("secrets.yml"))
        .stdout(Stdio::piped());
    let mut child = command.spawn().expect("start server");
    let stdout = child.stdout.take().expect("server stdout");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(stdout);
        let result = loop {
            let mut line = String::new();
            if let Err(error) = reader.read_line(&mut line) {
                break Err(error);
            }
            if line.is_empty() {
                break Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "server exited before emitting readiness",
                ));
            }
            if !line.trim().is_empty() {
                break Ok(line);
            }
        };
        sender.send(result).expect("send server readiness");
    });

    let line = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("server did not emit readiness within ten seconds")
        .expect("read server readiness");
    child.kill().expect("stop server");
    child.wait().expect("wait for server");

    let outcome: serde_json::Value = serde_json::from_str(&line).expect("parse JSON readiness");
    assert_eq!(outcome["result"], "server_ready");
    assert_eq!(outcome["address"], "0.0.0.0");
    assert!(outcome["port"].as_u64().is_some_and(|port| port > 0));
}
