#![cfg(feature = "server")]

use esdiag::{
    exporter::Exporter,
    server::{RuntimeMode, Server},
};
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;

async fn start_server(mode: RuntimeMode) -> (Server, Client, String) {
    let (server, bound_addr) =
        Server::start([127, 0, 0, 1], 0, Exporter::default(), String::new(), mode)
            .await
            .expect("start local server");

    let client = Client::new();
    let base = format!("http://127.0.0.1:{}", bound_addr.port());

    for _ in 0..40 {
        if client.get(format!("{base}/")).send().await.is_ok() {
            return (server, client, base);
        }
        sleep(Duration::from_millis(25)).await;
    }

    panic!("server did not become reachable in time");
}

#[tokio::test]
async fn service_mode_requires_iap_header_for_web_access() {
    let (mut server, client, base) = start_server(RuntimeMode::Service).await;

    let unauthorized = client
        .get(format!("{base}/"))
        .send()
        .await
        .expect("service mode request");
    assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED);

    let authorized = client
        .get(format!("{base}/"))
        .header(
            "X-Goog-Authenticated-User-Email",
            "accounts.google.com:ops@example.com",
        )
        .send()
        .await
        .expect("service mode authorized request");
    assert_eq!(authorized.status(), reqwest::StatusCode::OK);
    let body = authorized.text().await.expect("authorized body");
    assert!(body.contains("ops@example.com"));

    server.shutdown().await;
}

#[tokio::test]
async fn user_mode_allows_anonymous_web_access() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .get(format!("{base}/"))
        .send()
        .await
        .expect("user mode request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("user mode body");
    assert!(body.contains("Anonymous"));

    server.shutdown().await;
}
