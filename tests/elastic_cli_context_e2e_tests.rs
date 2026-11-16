// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use axum::{
    Json, Router,
    body::Body,
    extract::{OriginalUri, State},
    http::{HeaderMap, Method, Response, header},
    response::IntoResponse,
    routing::any,
};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
    sync::{Arc, Mutex},
};
use tokio::net::TcpListener;

#[derive(Clone, Default)]
struct Requests {
    authorizations: Arc<Mutex<Vec<String>>>,
    paths: Arc<Mutex<Vec<String>>>,
}

#[tokio::test(flavor = "multi_thread")]
async fn active_input_configured_output_and_saved_job_run_successfully() {
    let (source_url, source_requests, source_server) = mock_elasticsearch("source").await;
    let (output_url, output_requests, output_server) = mock_elasticsearch("output").await;
    let home = tempfile::TempDir::new().expect("temp home");
    let state_dir = home.path().join(".esdiag");
    fs::create_dir_all(&state_dir).expect("state directory");
    let config = home.path().join(".elasticrc.yml");
    fs::write(
        &config,
        format!(
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: {source_url}\n      auth:\n        api_key: $(env:PROD_KEY)\n  monitoring:\n    elasticsearch:\n      url: {output_url}\n      auth:\n        api_key: $(env:MONITORING_KEY)\n"
        ),
    )
    .expect("write config");

    let configured = run(
        home.path(),
        &config,
        &state_dir,
        &source_url,
        "source-key-one",
        "monitoring-key-one",
        &["output", "set", "monitoring"],
    );
    assert_success(&configured, "configure output");

    let first = run(
        home.path(),
        &config,
        &state_dir,
        &source_url,
        "source-key-one",
        "monitoring-key-one",
        &["process", ".es", "--save-job", "context-job"],
    );
    assert_success(&first, "process active context");
    assert!(String::from_utf8_lossy(&first.stdout).contains("diagnostic_processed"));

    let rerun = run(
        home.path(),
        &config,
        &state_dir,
        &source_url,
        "source-key-two",
        "monitoring-key-two",
        &["job", "run", "context-job"],
    );
    assert_success(&rerun, "run saved context job");

    let source_auth = source_requests.authorizations.lock().expect("source requests");
    assert!(source_auth.iter().any(|value| value == "ApiKey source-key-one"));
    assert!(source_auth.iter().any(|value| value == "ApiKey source-key-two"));
    let output_auth = output_requests.authorizations.lock().expect("output requests");
    assert!(output_auth.iter().any(|value| value == "ApiKey monitoring-key-one"));
    assert!(output_auth.iter().any(|value| value == "ApiKey monitoring-key-two"));
    assert!(
        output_requests
            .paths
            .lock()
            .expect("output paths")
            .iter()
            .any(|path| path.ends_with("/_bulk"))
    );

    source_server.abort();
    output_server.abort();
}

fn run(
    home: &Path,
    config: &Path,
    state_dir: &Path,
    source_url: &str,
    source_key: &str,
    output_key: &str,
    arguments: &[&str],
) -> Output {
    Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["--format", "json"])
        .args(arguments)
        .env("ESDIAG_ELASTIC_CLI", "1")
        .env("ELASTIC_CLI_CONFIG_FILE", config)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("ESDIAG_HOSTS", state_dir.join("hosts.yml"))
        .env("ELASTIC_ES_URL", source_url)
        .env("ELASTIC_ES_API_KEY", source_key)
        .env("PROD_KEY", source_key)
        .env("MONITORING_KEY", output_key)
        .output()
        .expect("run esdiag")
}

fn assert_success(output: &Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn mock_elasticsearch(cluster_name: &'static str) -> (String, Requests, tokio::task::JoinHandle<()>) {
    let requests = Requests::default();
    let app = Router::new()
        .fallback(any(elasticsearch_response))
        .with_state((cluster_name, requests.clone()));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock Elasticsearch");
    let address = listener.local_addr().expect("mock address");
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve mock Elasticsearch");
    });
    (format!("http://{address}"), requests, server)
}

async fn elasticsearch_response(
    State((cluster_name, requests)): State<(&'static str, Requests)>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
) -> Response<Body> {
    if let Some(value) = headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok()) {
        requests
            .authorizations
            .lock()
            .expect("authorization requests")
            .push(value.to_string());
    }
    requests
        .paths
        .lock()
        .expect("request paths")
        .push(uri.path().to_string());

    let value = if uri.path() == "/" {
        serde_json::json!({
            "name": format!("{cluster_name}-node"),
            "cluster_name": cluster_name,
            "cluster_uuid": format!("{cluster_name}-uuid"),
            "version": {
                "number": "9.3.3",
                "build_flavor": "default",
                "build_type": "tar",
                "build_hash": "0000000000000000000000000000000000000000",
                "build_date": "2026-01-01T00:00:00.000Z",
                "build_snapshot": false,
                "lucene_version": "10.0.0",
                "minimum_wire_compatibility_version": "8.19.0",
                "minimum_index_compatibility_version": "8.0.0"
            },
            "tagline": "You Know, for Search"
        })
    } else if uri.path().ends_with("/_bulk") && method == Method::POST {
        serde_json::json!({"errors": false, "items": [], "took": 1})
    } else if uri.path().contains("_cluster/health") {
        serde_json::json!({"cluster_name": cluster_name, "status": "green"})
    } else {
        serde_json::json!({})
    };
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert("x-elastic-product", header::HeaderValue::from_static("Elasticsearch"));
    response
}
