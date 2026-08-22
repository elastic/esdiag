// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! End-to-end parity coverage for the converged standalone `upload` command.

use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, head, post},
};
use std::{
    fs,
    io::Write,
    path::Path as FsPath,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::net::TcpListener;

#[derive(Clone, Default)]
struct UploadCalls {
    head: Arc<AtomicUsize>,
    put: Arc<AtomicUsize>,
    finalize: Arc<AtomicUsize>,
}

#[derive(Clone, Default)]
struct AgentBuilderRequests(Arc<Mutex<Option<serde_json::Value>>>);

async fn agent_builder_details() -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "id": "elastic-ai-agent",
        "name": "Diagnostic Agent"
    }))
}

async fn agent_builder_ask(State(requests): State<AgentBuilderRequests>, body: Bytes) -> impl IntoResponse {
    *requests.0.lock().expect("record Agent Builder request") =
        Some(serde_json::from_slice(&body).expect("Agent Builder JSON request"));
    (
        [(header::CONTENT_TYPE, "text/event-stream")],
        concat!(
            "event: conversation_id_set\n",
            "data: {\"data\":{\"conversation_id\":\"conv-123\"}}\n\n",
            "event: reasoning\n",
            "data: {\"data\":{\"reasoning\":\"Checking diagnostic\"}}\n\n",
            "event: message_complete\n",
            "data: {\"data\":{\"message_content\":\"Highest-risk finding\"}}\n\n",
            "event: round_complete\n",
            "data: {\"data\":{\"round\":{\"model_usage\":{\"input_tokens\":12,\"output_tokens\":3}}}}\n\n"
        ),
    )
}

async fn validate_upload(State(calls): State<UploadCalls>, Path(upload_id): Path<String>) -> StatusCode {
    assert_eq!(upload_id, "upload-id");
    calls.head.fetch_add(1, Ordering::SeqCst);
    StatusCode::OK
}

async fn receive_upload(State(calls): State<UploadCalls>, Path(upload_id): Path<String>, body: Bytes) -> StatusCode {
    assert_eq!(upload_id, "upload-id");
    assert!(!body.is_empty(), "archive upload must include bundle bytes");
    calls.put.fetch_add(1, Ordering::SeqCst);
    StatusCode::CREATED
}

async fn finalize_upload(
    State(calls): State<UploadCalls>,
    Path((upload_id, _digest)): Path<(String, String)>,
) -> (StatusCode, axum::Json<serde_json::Value>) {
    assert_eq!(upload_id, "upload-id");
    calls.finalize.fetch_add(1, Ordering::SeqCst);
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "slug": "uploaded-diagnostic",
            "token": "redacted"
        })),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn standalone_upload_uses_executor_sender_with_custom_api_url() {
    let calls = UploadCalls::default();
    let app = Router::new()
        .route("/api/uploads/{upload_id}", head(validate_upload).put(receive_upload))
        .route("/api/uploads/{upload_id}/{digest}/_finalize", post(finalize_upload))
        .with_state(calls.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind upload service");
    let address = listener.local_addr().expect("upload service address");
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve upload service");
    });
    let archive = tempfile::Builder::new()
        .suffix(".zip")
        .tempfile()
        .expect("archive file");
    let mut zip = zip::ZipWriter::new(archive.reopen().expect("archive handle"));
    zip.start_file("diagnostic.txt", zip::write::SimpleFileOptions::default())
        .expect("archive entry");
    zip.write_all(b"diagnostic archive").expect("archive contents");
    zip.finish().expect("finish archive");

    let output = tokio::task::spawn_blocking({
        let archive = archive.path().to_path_buf();
        move || {
            Command::new(env!("CARGO_BIN_EXE_esdiag"))
                .args([
                    "upload",
                    archive.to_str().expect("archive path"),
                    "upload-id",
                    "--api-url",
                    &format!("http://{address}"),
                ])
                .output()
                .expect("run esdiag upload")
        }
    })
    .await
    .expect("join CLI command");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("diagnostic_uploaded"));
    assert_eq!(calls.head.load(Ordering::SeqCst), 1);
    assert_eq!(calls.put.load(Ordering::SeqCst), 1);
    assert_eq!(calls.finalize.load(Ordering::SeqCst), 1);
    server.abort();
}

#[test]
fn process_archive_uses_executor_and_writes_selected_local_output() {
    let output_dir = tempfile::tempdir().expect("output directory");
    let output = output_dir.path().join("processed.ndjson");
    let archive = fixture_archive();

    let result = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["process", archive, output.to_str().expect("output path")])
        .output()
        .expect("run esdiag process");

    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        output.exists(),
        "the selected processed-document output must be written"
    );
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("diagnostic_processed"),
        "stdout: {}",
        String::from_utf8_lossy(&result.stdout)
    );
}

#[test]
fn process_directory_uses_executor_and_writes_selected_local_output() {
    let input = tempfile::tempdir().expect("extracted diagnostic directory");
    let output_dir = tempfile::tempdir().expect("output directory");
    let output = output_dir.path().join("processed.ndjson");
    extract_archive(FsPath::new(fixture_archive()), input.path());

    let result = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args([
            "process",
            input.path().to_str().expect("input path"),
            output.to_str().expect("output path"),
        ])
        .output()
        .expect("run esdiag process");

    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(output.exists(), "directory input must write processed output");
}

#[tokio::test(flavor = "multi_thread")]
async fn process_uses_environment_output_when_output_is_omitted() {
    async fn info() -> impl IntoResponse {
        axum::Json(serde_json::json!({
            "name": "mock-output",
            "version": { "number": "9.4.2" }
        }))
    }

    async fn bulk(body: Bytes) -> impl IntoResponse {
        let documents = body.iter().filter(|byte| **byte == b'\n').count() / 2;
        let items: Vec<_> = (0..documents)
            .map(|_| serde_json::json!({ "create": { "status": 201 } }))
            .collect();
        axum::Json(serde_json::json!({ "errors": false, "items": items }))
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind output Elasticsearch");
    let address = listener.local_addr().expect("output Elasticsearch address");
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/", get(info)).route("/{*path}", post(bulk)),
        )
        .await
        .expect("serve output Elasticsearch");
    });

    let output = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_esdiag"))
            .args(["process", fixture_archive()])
            .env("ESDIAG_OUTPUT_URL", format!("http://{address}"))
            .env_remove("ESDIAG_OUTPUT_APIKEY")
            .env_remove("ESDIAG_OUTPUT_USERNAME")
            .env_remove("ESDIAG_OUTPUT_PASSWORD")
            .output()
            .expect("run esdiag process with environment output")
    })
    .await
    .expect("join process command");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("diagnostic_processed"),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    server.abort();
}

#[cfg(feature = "agent")]
#[tokio::test(flavor = "multi_thread")]
async fn process_ask_sends_the_completed_diagnostic_to_its_output_deployment_agent() {
    async fn info() -> impl IntoResponse {
        axum::Json(serde_json::json!({
            "name": "mock-output",
            "version": { "number": "9.4.2" }
        }))
    }

    async fn bulk(body: Bytes) -> impl IntoResponse {
        let documents = body.iter().filter(|byte| **byte == b'\n').count() / 2;
        let items: Vec<_> = (0..documents)
            .map(|_| serde_json::json!({ "create": { "status": 201 } }))
            .collect();
        axum::Json(serde_json::json!({ "errors": false, "items": items }))
    }

    let output_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind output Elasticsearch");
    let output_address = output_listener.local_addr().expect("output address");
    let output_server = tokio::spawn(async move {
        axum::serve(
            output_listener,
            Router::new().route("/", get(info)).route("/{*path}", post(bulk)),
        )
        .await
        .expect("serve output Elasticsearch");
    });

    let requests = AgentBuilderRequests::default();
    let kibana_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind Kibana Agent Builder");
    let kibana_address = kibana_listener.local_addr().expect("Kibana address");
    let server_requests = requests.clone();
    let kibana_server = tokio::spawn(async move {
        axum::serve(
            kibana_listener,
            Router::new()
                .route(
                    "/s/esdiag/api/agent_builder/agents/elastic-ai-agent",
                    get(agent_builder_details),
                )
                .route("/s/esdiag/api/agent_builder/converse/async", post(agent_builder_ask))
                .with_state(server_requests),
        )
        .await
        .expect("serve Kibana Agent Builder");
    });

    let home = tempfile::tempdir().expect("temporary home");
    let esdiag_home = home.path().join(".esdiag");
    fs::create_dir_all(&esdiag_home).expect("create ESDiag home");
    let hosts = esdiag_home.join("hosts.yml");
    fs::write(
        &hosts,
        format!(
            "output-es:\n  auth: NoAuth\n  app: elasticsearch\n  roles:\n    - send\n  url: http://{output_address}\n  viewer: kibana\nkibana:\n  auth: NoAuth\n  app: kibana\n  roles:\n    - view\n  url: http://{kibana_address}\n"
        ),
    )
    .expect("write linked output deployment");

    let home_path = home.path().to_path_buf();
    let request_state = requests.clone();
    let result = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_esdiag"))
            .args([
                "--format",
                "json",
                "process",
                fixture_archive(),
                "output-es",
                "--ask",
                "What is the highest-risk finding?",
            ])
            .env("HOME", &home_path)
            .env("USERPROFILE", &home_path)
            .env("ESDIAG_HOSTS", &hosts)
            .env("ESDIAG_KEYSTORE", home_path.join(".esdiag").join("secrets.yml"))
            .env_remove("ESDIAG_OUTPUT_URL")
            .env_remove("ESDIAG_OUTPUT_APIKEY")
            .env_remove("ESDIAG_OUTPUT_USERNAME")
            .env_remove("ESDIAG_OUTPUT_PASSWORD")
            .output()
            .expect("run esdiag process --ask")
    })
    .await
    .expect("join process --ask command");

    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&result.stdout).expect("Agent Builder outcome");
    assert_eq!(outcome["result"], "agent_response");
    assert_eq!(outcome["conversation_id"], "conv-123");
    assert_eq!(outcome["message"], "Highest-risk finding");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("Diagnostic Agent: Checking diagnostic"),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("Agent Builder:"), "stderr: {stderr}");

    let request = request_state
        .0
        .lock()
        .expect("read Agent Builder request")
        .clone()
        .expect("Agent Builder request");
    assert_eq!(request["agent_id"], "elastic-ai-agent");
    let prompt = request["input"].as_str().expect("Agent Builder prompt");
    let (diagnostic, question) = prompt.split_once('\n').expect("diagnostic context and question");
    assert!(diagnostic.starts_with("diagnostic.id: "));
    assert_eq!(question, "What is the highest-risk finding?");

    output_server.abort();
    kibana_server.abort();
}

#[test]
fn explicit_process_output_overrides_environment_output() {
    let output_dir = tempfile::tempdir().expect("output directory");
    let output = output_dir.path().join("processed.ndjson");

    let result = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["process", fixture_archive(), output.to_str().expect("output path")])
        .env("ESDIAG_OUTPUT_URL", "http://127.0.0.1:9")
        .output()
        .expect("run esdiag process");

    assert!(
        result.status.success(),
        "explicit local output must take precedence: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(output.exists(), "explicit output must be used");
}

#[test]
fn process_failure_returns_nonzero_with_stage_summary() {
    let archive = tempfile::Builder::new()
        .suffix(".zip")
        .tempfile()
        .expect("empty diagnostic archive");
    zip::ZipWriter::new(archive.reopen().expect("archive handle"))
        .finish()
        .expect("finish archive");

    let result = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["process", archive.path().to_str().expect("archive path"), "-"])
        .output()
        .expect("run esdiag process");

    assert!(!result.status.success(), "invalid diagnostic must fail");
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("Process failed"),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn collect_and_process_save_job_persist_phase_jobs_before_execution() {
    let home = tempfile::tempdir().expect("temporary home");
    let esdiag_home = home.path().join(".esdiag");
    fs::create_dir_all(&esdiag_home).expect("create ESDiag home");
    let hosts = esdiag_home.join("hosts.yml");
    fs::write(
        &hosts,
        r#"saved-host:
  auth: NoAuth
  app: elasticsearch
  roles:
    - collect
    - send
  url: http://127.0.0.1:9
"#,
    )
    .expect("write known host");
    let output = tempfile::tempdir().expect("collection output");

    let collect = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args([
            "collect",
            "saved-host",
            output.path().to_str().expect("output path"),
            "--save-job",
            "saved-collect",
        ])
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("ESDIAG_HOSTS", &hosts)
        .output()
        .expect("run collect save-job");
    assert!(!collect.status.success(), "offline collection should fail after saving");

    let process = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["process", "saved-host", "saved-host", "--save-job", "saved-process"])
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("ESDIAG_HOSTS", &hosts)
        .output()
        .expect("run process save-job");
    assert!(!process.status.success(), "offline processing should fail after saving");

    let jobs = fs::read_to_string(esdiag_home.join("jobs.yml")).expect("saved jobs");
    assert!(jobs.contains("saved-collect"), "collect job must persist");
    assert!(jobs.contains("saved-process"), "process job must persist");
}

fn fixture_archive() -> &'static str {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/archives/elasticsearch-api-diagnostics-9.3.3.zip"
    )
}

fn extract_archive(archive: &FsPath, output: &FsPath) {
    let file = fs::File::open(archive).expect("open fixture archive");
    zip::ZipArchive::new(file)
        .expect("read fixture archive")
        .extract(output)
        .expect("extract fixture archive");
}
