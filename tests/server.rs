use axum::http::StatusCode;
use esdiag::server::Server;
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;
use tokio::time::sleep;

pub struct TestServer {
    pub server: Server,
    pub base_url: String,
}

/// Creates a test server instance and returns its address
pub async fn setup_test_server() -> TestServer {
    let port = portpicker::pick_unused_port().expect("No ports free");
    let server = Server::new(port, Default::default(), "http://localhost:5601".into());
    let base_url = format!("http://127.0.0.1:{}", port);

    // Give the server a moment to start
    sleep(Duration::from_millis(100)).await;

    TestServer { server, base_url }
}

/// Send a POST request to the test server with JSON payload
pub async fn post_json<T: Serialize>(
    base_url: &str,
    path: &str,
    payload: &T,
) -> Result<(StatusCode, String), Box<dyn std::error::Error>> {
    let client = Client::new();
    let url = format!("{}{}", base_url, path);

    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(payload)
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;

    Ok((status, body))
}

/// Send a GET request to the test server
pub async fn get(
    base_url: &str,
    path: &str,
) -> Result<(StatusCode, String), Box<dyn std::error::Error>> {
    let client = Client::new();
    let url = format!("{}{}", base_url, path);

    let response = client.get(url).send().await?;
    let status = response.status();
    let body = response.text().await?;

    Ok((status, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_server_setup() {
        let mut test_server = setup_test_server().await;

        // Test simple request to server
        let (status, _) = get(&test_server.base_url, "/").await.unwrap();
        assert_eq!(status, StatusCode::OK);

        // Clean up
        test_server.server.shutdown().await;
    }

    #[tokio::test]
    async fn test_json_request() {
        let mut test_server = setup_test_server().await;

        let payload = json!({
            "test": "data"
        });

        // This will fail with 404 since we're testing a non-existent endpoint,
        // but it verifies the JSON request functionality
        let (status, _) = post_json(&test_server.base_url, "/test", &payload)
            .await
            .unwrap();
        assert_eq!(status, StatusCode::NOT_FOUND);

        // Clean up
        test_server.server.shutdown().await;
    }
}
