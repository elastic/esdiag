#![cfg(all(feature = "server", feature = "desktop"))]

use esdiag::{
    exporter::Exporter,
    server::{RuntimeMode, Server},
};
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;

async fn start_server(mode: RuntimeMode) -> (Server, Client, String) {
    let (server, bound_addr) = Server::start([127, 0, 0, 1], 0, Exporter::default(), String::new(), mode)
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
async fn desktop_settings_modal_service_mode_disables_exporter_changes() {
    let (mut server, client, base) = start_server(RuntimeMode::Service).await;

    let response = client
        .get(format!("{base}/settings/modal"))
        .header("X-Goog-Authenticated-User-Email", "accounts.google.com:ops@example.com")
        .send()
        .await
        .expect("settings modal response");
    assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);

    server.shutdown().await;
}

#[tokio::test]
async fn desktop_settings_modal_user_mode_allows_host_management() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .get(format!("{base}/settings/modal"))
        .send()
        .await
        .expect("settings modal response");
    assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);

    server.shutdown().await;
}
