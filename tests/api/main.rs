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

#[tokio::test]
async fn status_with_auth_header_returns_user() {
    // Create a server instance
    let port = 9882;
    let exporter = "-".to_string(); // "-" uses stdout
    let mut server = ApiServer::new(port, exporter.clone());

    // Allow time for server to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create HTTP client
    let client = reqwest::Client::new();
    let status_url = format!("http://localhost:{}/status", port);

    // Test 1: Request without the auth header
    let status_response = client
        .get(&status_url)
        .send()
        .await
        .expect("Failed to send status request without header");

    let status_body: Value = status_response
        .json()
        .await
        .expect("Failed to parse status response");

    // Verify user field is null when header is missing
    assert_eq!(
        status_body["user"],
        Value::Null,
        "User field should be null when auth header is missing"
    );

    // Test 2: Request with the auth header
    let status_response = client
        .get(&status_url)
        .header(
            "X-Goog-Authenticated-User-Email",
            "accounts.google.com:test.user@example.com",
        )
        .send()
        .await
        .expect("Failed to send status request with header");

    let status_body: Value = status_response
        .json()
        .await
        .expect("Failed to parse status response");

    // Verify user field contains the extracted email
    assert_eq!(
        status_body["user"], "test.user@example.com",
        "User field should contain the extracted email from auth header"
    );

    // Clean up - properly shutdown the server and processor thread
    server.shutdown().await;
}

#[tokio::test]
async fn auth_header_filters_job_history() {
    // Create a server instance for testing
    let port = 9883;
    let mut server = ApiServer::new(port, "-".to_string());

    // Allow time for server to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create test client
    let client = reqwest::Client::new();
    let status_url = format!("http://localhost:{}/status", port);

    // Define test user emails
    let user1_email = "accounts.google.com:user1@example.com";
    let user2_email = "accounts.google.com:user2@example.com";

    // Manually add test jobs to the history
    {
        use esdiag::processor::JobFailed;

        // Add jobs with no user (visible without auth)
        server
            .job_record_failure(JobFailed {
                id: "job1".to_string(),
                filename: "public_file1.zip".to_string(),
                user: None,
                error: "Test error for public job 1".to_string(),
            })
            .await;

        server
            .job_record_failure(JobFailed {
                id: "job2".to_string(),
                filename: "public_file2.zip".to_string(),
                user: None,
                error: "Test error for public job 2".to_string(),
            })
            .await;

        // Add user-specific jobs
        server
            .job_record_failure(JobFailed {
                id: "job3".to_string(),
                filename: "user1_specific.zip".to_string(),
                user: Some("user1@example.com".to_string()),
                error: "Test error for user1".to_string(),
            })
            .await;

        server
            .job_record_failure(JobFailed {
                id: "job4".to_string(),
                filename: "user2_specific.zip".to_string(),
                user: Some("user2@example.com".to_string()),
                error: "Test error for user2".to_string(),
            })
            .await;
    }

    // Test 1: No auth header should show only jobs with no user
    let status_response = client
        .get(&status_url)
        .send()
        .await
        .expect("Failed to send status request without auth");

    let status_body: Value = status_response
        .json()
        .await
        .expect("Failed to parse status response");

    // Check that public jobs are visible (those with user: None)
    let history = &status_body["history"];
    assert!(history.is_array(), "History should be an array");
    assert_eq!(
        history.as_array().unwrap().len(),
        2,
        "Without auth header, should see jobs with no user"
    );

    // Test 2: User 1 auth header should only show User 1's job
    let status_response = client
        .get(&status_url)
        .header("X-Goog-Authenticated-User-Email", user1_email)
        .send()
        .await
        .expect("Failed to send status request with User 1 auth");

    let status_body: Value = status_response
        .json()
        .await
        .expect("Failed to parse status response");

    // With user1 auth, should see only user1's job
    let history = &status_body["history"];
    assert!(history.is_array(), "History should be an array");
    assert_eq!(
        history.as_array().unwrap().len(),
        1,
        "With User 1 auth header, should see only user1's job"
    );

    // Verify it's the correct job
    if !history.as_array().unwrap().is_empty() {
        let job = &history.as_array().unwrap()[0];
        if let Some(failed) = job.get("Failed") {
            if let Some(filename) = failed.get("filename") {
                if let Some(filename_str) = filename.as_str() {
                    assert!(
                        filename_str.contains("user1_specific.zip"),
                        "User1 should see their specific job"
                    );
                }
            }
        }
    }

    // Test 3: User 2 auth header should only show User 2's job
    let status_response = client
        .get(&status_url)
        .header("X-Goog-Authenticated-User-Email", user2_email)
        .send()
        .await
        .expect("Failed to send status request with User 2 auth");

    let status_body: Value = status_response
        .json()
        .await
        .expect("Failed to parse status response");

    // With user2 auth, should see only user2's job
    let history = &status_body["history"];
    assert!(history.is_array(), "History should be an array");
    assert_eq!(
        history.as_array().unwrap().len(),
        1,
        "With User 2 auth header, should see only user2's job"
    );

    // Verify it's the correct job
    if !history.as_array().unwrap().is_empty() {
        let job = &history.as_array().unwrap()[0];
        if let Some(failed) = job.get("Failed") {
            if let Some(filename) = failed.get("filename") {
                if let Some(filename_str) = filename.as_str() {
                    assert!(
                        filename_str.contains("user2_specific.zip"),
                        "User2 should see their specific job"
                    );
                }
            }
        }
    }

    // Clean up - properly shutdown the server
    server.shutdown().await;
}
