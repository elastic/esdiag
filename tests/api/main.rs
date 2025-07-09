use esdiag::server::ApiServer;
use reqwest::{StatusCode, multipart};
use serde_json::Value;
use std::time::Duration;

#[tokio::test]
async fn upload_non_zip_extension_returns_bad_request() {
    // Create a server instance
    let port = 9879;
    let exporter = "-".to_string(); // "-" uses stdout
    let _server = ApiServer::new(port, exporter.clone());

    // Allow server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create HTTP client
    let client = reqwest::Client::new();
    let url = format!("http://localhost:{}/upload", port);

    // Create a text file with non-ZIP extension
    let file_content = b"This is not a ZIP file";

    // Create form part with non-ZIP extension
    let file_part = multipart::Part::bytes(file_content.to_vec())
        .file_name("test_file.txt") // Non-ZIP extension
        .mime_str("text/plain")
        .unwrap();

    let form = multipart::Form::new().part("file", file_part);

    // Send the upload request
    let response = client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .expect("Failed to send request");

    // We expect a BAD_REQUEST status code since non-ZIP files should be rejected immediately
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Parse the response body
    let body: Value = response
        .json()
        .await
        .expect("Failed to parse JSON response");

    // The error message should indicate invalid file type
    assert_eq!(
        body["error"],
        "Invalid file type. Only .zip files are allowed."
    );
}

#[tokio::test]
async fn upload_without_filename_returns_bad_request() {
    // Create a server instance
    let port = 9880;
    let exporter = "-".to_string(); // "-" uses stdout
    let _server = ApiServer::new(port, exporter.clone());

    // Allow server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create HTTP client
    let client = reqwest::Client::new();
    let url = format!("http://localhost:{}/upload", port);

    // Create form part with no filename
    let file_part = multipart::Part::bytes(vec![1, 2, 3, 4])
        // No filename provided
        .mime_str("application/octet-stream")
        .unwrap();

    let form = multipart::Form::new().part("file", file_part);

    // Send the upload request
    let response = client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .expect("Failed to send request");

    // We expect a BAD_REQUEST status code since no filename was provided
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Parse the response body
    let body: Value = response
        .json()
        .await
        .expect("Failed to parse JSON response");

    // The error message should indicate no filename
    assert_eq!(body["error"], "No file name provided");
}

#[tokio::test]
async fn upload_invalid_zip_processes_and_returns_ready() {
    // Create a server instance
    let port = 9881;
    let exporter = "-".to_string(); // "-" uses stdout
    let mut server = ApiServer::new(port, exporter.clone());

    // Allow time for server to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create HTTP client
    let client = reqwest::Client::new();
    let url = format!("http://localhost:{}/upload", port);

    // Create fake data that's not a valid ZIP file but has .zip extension
    let file_content = b"This is not a valid ZIP file content";

    // Create form part with ZIP extension but invalid content
    let file_part = multipart::Part::bytes(file_content.to_vec())
        .file_name("invalid_content.zip") // Has ZIP extension but content isn't valid
        .mime_str("application/octet-stream")
        .unwrap();

    let form = multipart::Form::new().part("file", file_part);

    // Send the upload request
    let response = client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .expect("Failed to send request");

    // The server accepts files with .zip extension initially
    assert_eq!(response.status(), StatusCode::OK);

    // Parse the response body
    let body: Value = response
        .json()
        .await
        .expect("Failed to parse JSON response");

    // The response should indicate processing has started
    assert_eq!(
        body["status"], "processing",
        "Upload response status should be 'processing'"
    );
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("invalid_content.zip"),
        "Upload response message should include the filename"
    );

    // Wait for job processing to complete
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Check the status endpoint
    let status_url = format!("http://localhost:{}/status", port);
    let status_response = client
        .get(&status_url)
        .send()
        .await
        .expect("Failed to send status request");

    let status_body: Value = status_response
        .json()
        .await
        .expect("Failed to parse status response");

    // After processing, the server should be in "ready" state regardless of job success/failure
    // With the job queue implementation, the job will be processed by the background thread
    // and the job status will be recorded in history (visible in browser tests)
    assert_eq!(
        status_body["status"], "ready",
        "Server should be in 'ready' state after processing"
    );

    // Queue should be empty since the job should have been processed
    assert_eq!(
        status_body["queue"]["size"], 0,
        "Queue should be empty after processing"
    );

    // Clean up - properly shutdown the server and processor thread
    server.shutdown().await;
}
