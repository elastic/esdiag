mod server;
use axum::http::StatusCode;
use serde_json::{Value, json};
use server::{post_json, setup_test_server};

#[tokio::test]
async fn valid_service_link_returns_http_201() {
    let mut test_server = setup_test_server().await;

    // Valid request payload
    let payload = json!({
        "url": "https://upload.elastic.co/example",
        "token": "test_token",
        "metadata": {
            "user": "test@elastic.co",
        }
    });

    let (status, body) = post_json(&test_server.base_url, "/api/service_link", &payload)
        .await
        .unwrap();

    assert_eq!(status, StatusCode::CREATED);

    let body: Value = serde_json::from_str(&body).unwrap();
    assert!(
        body.get("link_id").is_some(),
        "Response should contain a link_id"
    );
    assert!(body["link_id"].is_u64(), "link_id should be a number");

    // Clean up
    test_server.server.shutdown().await;
}

#[tokio::test]
async fn invalid_url_returns_http_400() {
    let mut test_server = setup_test_server().await;

    // Invalid URL in request
    let payload = json!({
        "url": "not_a_valid_url",
        "token": "test_token",
        "metadata": {
            "user": "test@elastic.co",
        }
    });

    let (status, body) = post_json(&test_server.base_url, "/api/service_link", &payload)
        .await
        .unwrap();

    assert_eq!(status, StatusCode::BAD_REQUEST);

    let body: Value = serde_json::from_str(&body).unwrap();
    assert!(
        body.get("error").is_some(),
        "Response should contain an error message"
    );
    assert!(
        body["error"].as_str().unwrap().contains("Invalid URL"),
        "Error should indicate invalid URL"
    );

    // Clean up
    test_server.server.shutdown().await;
}

#[tokio::test]
async fn non_elastic_upload_service_url_returns_http_400() {
    let mut test_server = setup_test_server().await;

    // URL that isn't upload.elastic.co
    let payload = json!({
        "url": "https://example.com/upload",
        "token": "test_token",
        "metadata": {
            "user": "test@elastic.co",
        }
    });

    let (status, body) = post_json(&test_server.base_url, "/api/service_link", &payload)
        .await
        .unwrap();

    // The service should reject URLs that are not `https://upload.elastic.co`
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let body: Value = serde_json::from_str(&body).unwrap();
    assert!(
        body.get("error").is_some(),
        "Response should contain an error message"
    );

    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("URL must be for the Elastic Upload Service"),
        "Error message should indicate invalid URL"
    );

    // Clean up
    test_server.server.shutdown().await;
}

#[tokio::test]
async fn missing_token_returns_http_422() {
    let mut test_server = setup_test_server().await;

    // Request without a token
    let payload = json!({
        "url": "https://upload.elastic.co/example",
        "metadata": {
            "user": "test@elastic.co"
        }
    });

    let (status, body) = post_json(&test_server.base_url, "/api/service_link", &payload)
        .await
        .unwrap();

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let body: Value = match serde_json::from_str(&body) {
        Ok(body) => body,
        Err(err) => json!({
            "error": format!("Failed to parse JSON response: {}", err)
        }),
    };

    assert!(
        body.get("error").is_some(),
        "Response should contain an error message"
    );

    // Clean up
    test_server.server.shutdown().await;
}

#[tokio::test]
async fn blank_token_returns_http_400() {
    let mut test_server = setup_test_server().await;

    // Request without a token
    let payload = json!({
        "url": "https://upload.elastic.co/example",
        "token": "",
        "metadata": {
            "user": "test@elastic.co",
        }
    });

    let (status, body) = post_json(&test_server.base_url, "/api/service_link", &payload)
        .await
        .unwrap();

    assert_eq!(status, StatusCode::BAD_REQUEST);

    let body: Value = serde_json::from_str(&body).unwrap();
    assert!(
        body.get("error").is_some(),
        "Response should contain an error message"
    );

    // Clean up
    test_server.server.shutdown().await;
}
