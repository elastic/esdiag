mod server;
use axum::http::StatusCode;
use serde_json::{Value, json};
use server::{post_json, setup_test_server};

#[tokio::test]
async fn api_key_valid_returns_http_201() {
    let mut test_server = setup_test_server().await;

    // Valid request payload
    let payload = json!({
        "url": "https://elasticsearch.example.com:9200",
        "apikey": "test_api_key_value",
        "metadata": {
            "user": "test@elastic.co",
        }
    });

    let (status, body) = post_json(&test_server.base_url, "/api/api_key", &payload)
        .await
        .unwrap();

    assert_eq!(status, StatusCode::CREATED);

    let body: Value = serde_json::from_str(&body).unwrap();
    assert!(
        body.get("key_id").is_some(),
        "Response should contain a key_id"
    );
    assert!(body["key_id"].is_u64(), "key_id should be a number");

    // Clean up
    test_server.server.shutdown().await;
}

#[tokio::test]
async fn api_key_invalid_url_returns_http_400() {
    let mut test_server = setup_test_server().await;

    // Invalid URL in request
    let payload = json!({
        "url": "not_a_valid_url",
        "apikey": "test_api_key_value",
        "metadata": {
            "user": "test@elastic.co",
        }
    });

    let (status, body) = post_json(&test_server.base_url, "/api/api_key", &payload)
        .await
        .unwrap();

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
            .contains("Failed to parse URL"),
        "Error should indicate invalid URL"
    );

    // Clean up
    test_server.server.shutdown().await;
}

#[tokio::test]
async fn api_key_missing_returns_http_422() {
    let mut test_server = setup_test_server().await;

    // Request without an apikey
    let payload = json!({
        "url": "https://elasticsearch.example.com:9200",
        "metadata": {
            "user": "test@elastic.co"
        }
    });

    let (status, body) = post_json(&test_server.base_url, "/api/api_key", &payload)
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
async fn api_key_blank_returns_http_400() {
    let mut test_server = setup_test_server().await;

    // Request with empty apikey
    let payload = json!({
        "url": "https://elasticsearch.example.com:9200",
        "apikey": "",
        "metadata": {
            "user": "test@elastic.co",
        }
    });

    let (status, body) = post_json(&test_server.base_url, "/api/api_key", &payload)
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

#[tokio::test]
async fn api_key_only_returns_http_400() {
    let mut test_server = setup_test_server().await;

    // Request with whitespace-only apikey
    let payload = json!({
        "url": "https://elasticsearch.example.com:9200",
        "apikey": "   ",
        "metadata": {
            "user": "test@elastic.co",
        }
    });

    let (status, body) = post_json(&test_server.base_url, "/api/api_key", &payload)
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

#[tokio::test]
async fn service_link_valid_returns_http_201() {
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
async fn service_link_invalid_url_returns_http_400() {
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
async fn service_link_non_elastic_upload_service_url_returns_http_400() {
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
async fn service_link_missing_token_returns_http_422() {
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
async fn service_link_blank_token_returns_http_400() {
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
