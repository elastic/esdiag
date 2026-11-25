// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use chrono::Utc;
use esdiag::es_mock::ElasticsearchMock;
use std::fs;
use std::process::Command;

#[test]
fn cli_creates_no_log_file_by_default_and_writes_daily_log_and_gzips_prior_when_logs_path_set() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).expect("home");

    // Default run with ESDIAG_LOGS_PATH unset: no log file created.
    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["--debug", "host", "list"])
        .env("ESDIAG_HOME", &home)
        .env_remove("ESDIAG_LOGS_PATH")
        .env_remove("ESDIAG_LOGS_URL")
        .env_remove("ESDIAG_LOGS_HOST")
        .output()
        .expect("run esdiag");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !home.join("last_run").join("esdiag.log").exists(),
        "default run must not create last_run/esdiag.log"
    );
    assert!(
        !home.join("logs").exists(),
        "default run must not create ~/.esdiag/logs"
    );

    // Opt-in run with ESDIAG_LOGS_PATH set: writes today's daily log and gzips prior day's log on exit.
    let logs_dir = tmp.path().join("custom-logs");
    fs::create_dir_all(&logs_dir).expect("logs_dir");
    let prior_log = logs_dir.join("esdiag-2026-01-01.log");
    fs::write(&prior_log, "prior day entry\n").expect("write prior log");

    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["--debug", "host", "list"])
        .env("ESDIAG_HOME", &home)
        .env("ESDIAG_LOGS_PATH", &logs_dir)
        .env("ESDIAG_LOGS_RETENTION_DAYS", "3650")
        .env_remove("ESDIAG_LOGS_URL")
        .env_remove("ESDIAG_LOGS_HOST")
        .output()
        .expect("run esdiag with logs path");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let today_name = format!("esdiag-{}.log", Utc::now().date_naive().format("%Y-%m-%d"));
    let today_log = logs_dir.join(&today_name);
    assert!(today_log.exists(), "expected daily log {}", today_log.display());
    assert!(
        !prior_log.exists(),
        "prior day's uncompressed log must be removed after exit gzip"
    );
    assert!(
        logs_dir.join("esdiag-2026-01-01.log.gz").exists(),
        "prior day's log must be gzipped on exit"
    );
}

#[tokio::test]
async fn cli_flushes_tracing_logs_to_remote_elasticsearch_before_exit() {
    let mock = ElasticsearchMock::new().require_authorization("ApiKey cli-logs-key");
    let base_url = mock.serve().await;
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).expect("home");

    let status = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_esdiag"))
            .args(["--debug", "host", "list"])
            .env("ESDIAG_HOME", &home)
            .env("ESDIAG_LOGS_URL", &base_url)
            .env("ESDIAG_LOGS_APIKEY", "cli-logs-key")
            .env("ESDIAG_LOGS_DATA_STREAM", "logs-esdiag-test")
            .env_remove("ESDIAG_LOGS_USERNAME")
            .env_remove("ESDIAG_LOGS_PASSWORD")
            .env_remove("ESDIAG_LOGS_INSECURE")
            .env_remove("ESDIAG_LOGS_HOST")
            .env_remove("ESDIAG_LOGS_PATH")
            .output()
            .expect("run esdiag")
    })
    .await
    .expect("join");

    assert!(
        status.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&status.stderr)
    );

    let requests = mock.bulk_requests();
    assert!(!requests.is_empty(), "expected at least one flushed _bulk request");
    assert!(requests.iter().all(|req| req.path_index == "logs-esdiag-test"));
    assert!(
        requests
            .iter()
            .all(|req| req.authorization.as_deref() == Some("ApiKey cli-logs-key"))
    );
    assert!(
        requests
            .iter()
            .flat_map(|req| req.actions.iter())
            .all(|action| action.op == "create" && action.pipeline.is_none())
    );
    assert!(
        requests
            .iter()
            .flat_map(|req| req.documents.iter())
            .any(|doc| doc["service"]["name"] == "esdiag" && doc["@timestamp"].is_string())
    );
}
