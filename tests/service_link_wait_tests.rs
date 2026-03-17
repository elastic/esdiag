// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

#![cfg(feature = "server")]

//! Integration tests for `wait_for_completion` on the `/api/service_link` endpoint.
//!
//! Tests marked `#[ignore]` require real Elastic Upload Service credentials. Set
//! `ESDIAG_TEST_UPLOAD_TOKEN` and `ESDIAG_TEST_UPLOAD_URL` before running them:
//!
//! ```text
//! ESDIAG_TEST_UPLOAD_TOKEN=<token> \
//! ESDIAG_TEST_UPLOAD_URL=https://upload.elastic.co/d/<id> \
//!     cargo test -- --ignored
//! ```

use axum::{Router, body::Body, extract::State, http::StatusCode, response::Response};
use bytes::Bytes;
use esdiag::{
    data::Uri,
    exporter::Exporter,
    processor::{Identifiers, Processor},
    receiver::Receiver,
    server::{RuntimeMode, Server},
};
use reqwest::Client;
use std::{io::Cursor, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::oneshot, time::sleep};
use url::Url;

const UPLOAD_FILENAME: &str = "elasticsearch-api-diagnostics-9.1.3.zip";

/// Returns real upload credentials from environment variables for `#[ignore]`d
/// network tests. Panics with a helpful message if the variables are unset.
fn upload_credentials() -> (String, String) {
    let token = std::env::var("ESDIAG_TEST_UPLOAD_TOKEN")
        .expect("set ESDIAG_TEST_UPLOAD_TOKEN to run ignored network tests");
    let url = std::env::var("ESDIAG_TEST_UPLOAD_URL")
        .expect("set ESDIAG_TEST_UPLOAD_URL to run ignored network tests");
    (token, url)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct MockState {
    zip_bytes: Arc<Bytes>,
    token: Arc<String>,
}

/// Axum handler for the mock upload service.
///
/// Returns the zip bytes when the `Authorization` header matches the expected
/// token, mirroring the authentication contract of the real Elastic Upload
/// Service.
async fn mock_upload_handler(
    State(state): State<MockState>,
    headers: axum::http::HeaderMap,
) -> Response {
    let auth_ok = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == state.token.as_str())
        .unwrap_or(false);

    if auth_ok {
        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/zip")
            .body(Body::from((*state.zip_bytes).clone()))
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::empty())
            .unwrap()
    }
}

/// Starts a local HTTP server that serves `zip_bytes` at any path when the
/// correct `Authorization` header is present.
///
/// Returns the bound address and a shutdown sender. Drop or send on the sender
/// to stop the server.
async fn start_mock_upload_server(
    zip_bytes: Bytes,
    token: &str,
) -> (SocketAddr, oneshot::Sender<()>) {
    let state = MockState {
        zip_bytes: Arc::new(zip_bytes),
        token: Arc::new(token.to_string()),
    };

    let app = Router::new()
        .fallback(mock_upload_handler)
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock server");
    let addr = listener.local_addr().expect("mock server addr");

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                shutdown_rx.await.ok();
            })
            .await
            .expect("mock server error");
    });

    (addr, shutdown_tx)
}

/// Downloads the diagnostic zip from the Elastic Upload Service using
/// `reqwest` — the same `Authorization` header pattern used by
/// `UploadServiceDownloader` internally.
///
/// Only used by `#[ignore]`d tests that require real network access.
async fn download_from_upload_service() -> Bytes {
    let (token, url) = upload_credentials();
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("Authorization", token)
        .send()
        .await
        .expect("request to upload service should succeed");
    assert!(
        response.status().is_success(),
        "upload service returned {}: link may have expired",
        response.status()
    );
    let bytes = response.bytes().await.expect("read response bytes");
    assert!(!bytes.is_empty(), "downloaded zip should not be empty");
    bytes
}

async fn start_esdiag_server() -> (Server, Client, String) {
    let (server, bound_addr) = Server::start(
        [127, 0, 0, 1],
        0,
        Exporter::default(),
        String::new(),
        RuntimeMode::User,
    )
    .await
    .expect("start esdiag server");

    let client = Client::new();
    let base = format!("http://127.0.0.1:{}", bound_addr.port());

    for _ in 0..40 {
        if client.get(format!("{base}/favicon.ico")).send().await.is_ok() {
            return (server, client, base);
        }
        sleep(Duration::from_millis(25)).await;
    }

    panic!("esdiag server did not become reachable");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Creates a valid but empty ZIP archive in memory (no entries).
///
/// Useful for tests that need a receiver to succeed but processing to fail,
/// since an empty zip will parse as a valid archive but contain no diagnostic files.
fn empty_zip_bytes() -> Bytes {
    let mut buf = Cursor::new(Vec::<u8>::new());
    zip::ZipWriter::new(&mut buf)
        .finish()
        .expect("write empty zip");
    Bytes::from(buf.into_inner())
}

/// Exercises the full Receiver → Processor pipeline that `wait_for_completion=true`
/// invokes on `/api/service_link`.
///
/// Downloads the real diagnostic zip from the Elastic Upload Service, serves it
/// from a local mock HTTP server, then builds `Uri::ServiceLink` directly (bypassing
/// the `upload.elastic.co` domain guard that lives only in the HTTP routing layer)
/// to drive `UploadServiceDownloader` against the mock.
///
/// Requires network access to `upload.elastic.co`; the upload link will expire
/// after its TTL. Run explicitly with:
/// `cargo test -- --ignored service_link_wait_for_completion_processes_synchronously`
#[ignore = "requires network access to download the real diagnostic zip"]
#[tokio::test(flavor = "multi_thread")]
async fn service_link_wait_for_completion_processes_synchronously() {
    let zip_bytes = download_from_upload_service().await;
    let mock_token = "mock-token";
    let (mock_addr, shutdown_tx) = start_mock_upload_server(zip_bytes, mock_token).await;

    // Credentials are embedded as `token:<value>@host` — the convention expected
    // by UploadServiceDownloader::try_from, which extracts the password as the
    // Authorization header value and strips credentials before sending the GET.
    let mock_url = Url::parse(&format!(
        "http://token:{mock_token}@{mock_addr}/d/mock-diagnostic"
    ))
    .expect("valid mock URL");

    // Construct Uri::ServiceLink directly so the upload service receiver path
    // is exercised without going through the HTTP endpoint's domain validation.
    let uri = Uri::ServiceLink(mock_url);

    let receiver = Arc::new(
        Receiver::try_from(uri).expect("receiver should be created from mock upload service"),
    );

    let exporter = Arc::new(Exporter::default());
    let identifiers = Identifiers::new(
        Some("test-account".to_string()),
        Some("12345".to_string()),
        Some("mock-diagnostic.zip".to_string()),
        None,
        Some("test@example.com".to_string()),
    );

    let processor = Processor::try_new(receiver, exporter, identifiers)
        .await
        .expect("processor should initialise from downloaded zip");

    let processing = processor
        .start()
        .await
        .map_err(|f| format!("processor failed to start: {}", f.state.error))
        .expect("processor should start");
    let completed = processing
        .process()
        .await
        .map_err(|f| format!("processor failed: {}", f.state.error))
        .expect("processor should complete");

    assert!(
        !completed.state.report.diagnostic.metadata.id.is_empty(),
        "diagnostic_id should be populated after processing"
    );
    assert!(
        completed.state.runtime > 0,
        "processing runtime should be positive"
    );

    let _ = shutdown_tx.send(());
}

/// Verifies that omitting `wait_for_completion` uses the async path and returns
/// HTTP 201 with a `link_id`.
#[tokio::test(flavor = "multi_thread")]
async fn service_link_defaults_to_async_when_no_wait_for_completion_param() {
    let (mut server, client, base) = start_esdiag_server().await;

    let response = client
        .post(format!("{base}/api/service_link"))
        .json(&serde_json::json!({
            "metadata": { "account": "test-account", "case_number": "12345" },
            "token": "mock-token",
            "url": "https://upload.elastic.co/d/test"
        }))
        .send()
        .await
        .expect("POST /api/service_link should succeed");

    assert_eq!(
        response.status(),
        reqwest::StatusCode::CREATED,
        "missing wait_for_completion should return 201 Created"
    );

    let body: serde_json::Value = response.json().await.expect("response should be JSON");
    assert!(
        body["link_id"].is_number(),
        "response should contain numeric link_id, got: {body}"
    );

    server.shutdown().await;
}

/// Verifies that `wait_for_completion=false` explicitly selects the async path
/// and returns HTTP 201 with a `link_id`.
#[tokio::test(flavor = "multi_thread")]
async fn service_link_async_when_wait_for_completion_is_false() {
    let (mut server, client, base) = start_esdiag_server().await;

    let response = client
        .post(format!("{base}/api/service_link?wait_for_completion=false"))
        .json(&serde_json::json!({
            "metadata": { "account": "test-account", "case_number": "12345" },
            "token": "mock-token",
            "url": "https://upload.elastic.co/d/test"
        }))
        .send()
        .await
        .expect("POST /api/service_link should succeed");

    assert_eq!(
        response.status(),
        reqwest::StatusCode::CREATED,
        "wait_for_completion=false should return 201 Created"
    );

    let body: serde_json::Value = response.json().await.expect("response should be JSON");
    assert!(
        body["link_id"].is_number(),
        "response should contain numeric link_id, got: {body}"
    );

    server.shutdown().await;
}

/// Verifies that `Receiver::try_from` returns an error when the upload service
/// returns non-zip bytes, covering the receiver creation failure branch in the
/// `wait_for_completion` sync path.
#[tokio::test(flavor = "multi_thread")]
async fn receiver_fails_when_download_returns_non_zip_bytes() {
    let bad_bytes = Bytes::from_static(b"this is not a zip file");
    let mock_token = "mock-token";
    let (mock_addr, shutdown_tx) = start_mock_upload_server(bad_bytes, mock_token).await;

    let mock_url =
        Url::parse(&format!("http://token:{mock_token}@{mock_addr}/d/mock-diagnostic"))
            .expect("valid mock URL");
    let uri = Uri::ServiceLink(mock_url);

    let result = Receiver::try_from(uri);
    assert!(
        result.is_err(),
        "Receiver::try_from should fail when download returns non-zip bytes"
    );

    let _ = shutdown_tx.send(());
}

/// Verifies that the processor returns an error when given a valid zip that
/// contains no diagnostic files, covering the processing failure branch in the
/// `wait_for_completion` sync path.
#[tokio::test(flavor = "multi_thread")]
async fn processor_fails_when_zip_contains_no_diagnostic_files() {
    let mock_token = "mock-token";
    let (mock_addr, shutdown_tx) =
        start_mock_upload_server(empty_zip_bytes(), mock_token).await;

    let mock_url =
        Url::parse(&format!("http://token:{mock_token}@{mock_addr}/d/mock-diagnostic"))
            .expect("valid mock URL");
    let uri = Uri::ServiceLink(mock_url);

    let receiver = Arc::new(
        Receiver::try_from(uri).expect("receiver should be created from a valid (empty) zip"),
    );

    let exporter = Arc::new(Exporter::default());
    let identifiers = Identifiers::new(
        Some("test-account".to_string()),
        Some("12345".to_string()),
        Some("empty.zip".to_string()),
        None,
        Some("test@example.com".to_string()),
    );

    // An empty zip has no diagnostic files: the pipeline will fail at try_new,
    // start, or process — all of which the handler maps to a 500 error response.
    let error_occurred = match Processor::try_new(receiver, exporter, identifiers).await {
        Err(_) => true,
        Ok(processor) => {
            match processor
                .start()
                .await
                .map_err(|f| f.state.error.to_string())
            {
                Err(_) => true,
                Ok(processing) => processing
                    .process()
                    .await
                    .map_err(|f| f.state.error.to_string())
                    .is_err(),
            }
        }
    };

    assert!(
        error_occurred,
        "pipeline should fail at some stage for a zip with no diagnostic files"
    );

    let _ = shutdown_tx.send(());
}

/// Tests the full `/api/service_link?wait_for_completion` HTTP endpoint.
///
/// Sends the real Elastic Upload Service URL and token. The ESDiag server
/// downloads directly from `upload.elastic.co` and returns the diagnostic
/// result synchronously.
///
/// Requires network access and valid upload credentials; the upload link will
/// expire after its TTL. Run explicitly with:
/// `cargo test -- --ignored service_link_endpoint_returns_diagnostic_when_wait_for_completion`
#[ignore = "requires network access and valid upload credentials"]
#[tokio::test(flavor = "multi_thread")]
async fn service_link_endpoint_returns_diagnostic_when_wait_for_completion() {
    let (token, url) = upload_credentials();
    let (mut server, client, base) = start_esdiag_server().await;

    let response = client
        .post(format!("{base}/api/service_link?wait_for_completion"))
        .json(&serde_json::json!({
            "metadata": {
                "account": "test-account",
                "case_number": "12345",
                "filename": UPLOAD_FILENAME
            },
            "token": token,
            "url": url
        }))
        .send()
        .await
        .expect("POST /api/service_link request should succeed");

    assert_eq!(
        response.status(),
        reqwest::StatusCode::OK,
        "wait_for_completion=true should return 200 OK"
    );

    let body: serde_json::Value = response.json().await.expect("response should be JSON");

    assert!(
        body["diagnostic_id"].as_str().is_some_and(|s| !s.is_empty()),
        "response should contain non-empty diagnostic_id, got: {body}"
    );
    assert!(
        body["kibana_link"].is_string(),
        "response should contain kibana_link, got: {body}"
    );
    assert!(
        body["took"].is_number(),
        "response should contain took (milliseconds), got: {body}"
    );

    server.shutdown().await;
}
