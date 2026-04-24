#![cfg(feature = "server")]

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
        .header("X-Goog-Authenticated-User-Email", "accounts.google.com:ops@example.com")
        .send()
        .await
        .expect("service mode authorized request");
    assert_eq!(authorized.status(), reqwest::StatusCode::OK);
    let body = authorized.text().await.expect("authorized body");
    assert!(body.contains("ops@example.com"));

    server.shutdown().await;
}

#[tokio::test]
async fn service_mode_does_not_mount_workflow_or_jobs_routes() {
    let (mut server, client, base) = start_server(RuntimeMode::Service).await;

    let workflow_response = client
        .get(format!("{base}/workflow"))
        .header("X-Goog-Authenticated-User-Email", "accounts.google.com:ops@example.com")
        .send()
        .await
        .expect("service mode workflow request");
    assert_eq!(workflow_response.status(), reqwest::StatusCode::NOT_FOUND);

    let jobs_response = client
        .get(format!("{base}/jobs"))
        .header("X-Goog-Authenticated-User-Email", "accounts.google.com:ops@example.com")
        .send()
        .await
        .expect("service mode jobs request");
    assert_eq!(jobs_response.status(), reqwest::StatusCode::NOT_FOUND);

    server.shutdown().await;
}

#[tokio::test]
async fn service_mode_requires_iap_header_for_settings_update() {
    let (mut server, client, base) = start_server(RuntimeMode::Service).await;

    let unauthorized = client
        .post(format!("{base}/api/settings/update"))
        .body(r#"{"settings":{"kibana_url":"https://kibana.example"}}"#)
        .header("content-type", "application/json")
        .send()
        .await
        .expect("service mode settings update without header");
    assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED);

    let authorized = client
        .post(format!("{base}/api/settings/update"))
        .body(r#"{"settings":{"kibana_url":"https://kibana.example"}}"#)
        .header("content-type", "application/json")
        .header("X-Goog-Authenticated-User-Email", "accounts.google.com:ops@example.com")
        .send()
        .await
        .expect("service mode settings update with header");
    assert_ne!(authorized.status(), reqwest::StatusCode::UNAUTHORIZED);

    server.shutdown().await;
}

#[tokio::test]
async fn service_mode_does_not_mount_keystore_routes() {
    let (mut server, client, base) = start_server(RuntimeMode::Service).await;

    let response = client
        .post(format!("{base}/keystore/unlock"))
        .form(&[("password", "pw")])
        .send()
        .await
        .expect("service mode keystore request without header");
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    let response = client
        .post(format!("{base}/keystore/unlock"))
        .header("X-Goog-Authenticated-User-Email", "accounts.google.com:ops@example.com")
        .form(&[("password", "pw")])
        .send()
        .await
        .expect("service mode keystore request");
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    let response = client
        .get(format!("{base}/keystore/modal"))
        .header("X-Goog-Authenticated-User-Email", "accounts.google.com:ops@example.com")
        .send()
        .await
        .expect("service mode keystore modal request");
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    server.shutdown().await;
}

#[cfg(feature = "keystore")]
#[tokio::test]
async fn user_mode_mounts_keystore_routes_when_feature_enabled() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .get(format!("{base}/keystore/modal"))
        .send()
        .await
        .expect("user mode keystore modal request");
    assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);

    server.shutdown().await;
}

#[cfg(not(feature = "keystore"))]
#[tokio::test]
async fn user_mode_does_not_mount_keystore_routes_when_feature_disabled() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .post(format!("{base}/keystore/unlock"))
        .form(&[("password", "pw")])
        .send()
        .await
        .expect("user mode keystore unlock request");
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    let response = client
        .get(format!("{base}/keystore/modal"))
        .send()
        .await
        .expect("user mode keystore modal request");
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    server.shutdown().await;
}

#[tokio::test]
async fn user_mode_allows_anonymous_web_access() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client.get(format!("{base}/")).send().await.expect("user mode request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("user mode body");
    assert!(body.contains("Anonymous"));
    assert!(body.contains("Process Diagnostics"));
    assert!(!body.contains("id=\"workflow-go-button\""));

    server.shutdown().await;
}

#[tokio::test]
async fn user_mode_workflow_shows_known_host_collect_and_local_save_defaults() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .get(format!("{base}/workflow"))
        .send()
        .await
        .expect("user mode request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("user mode body");

    assert!(body.contains("id=\"collect-known-host\""));
    assert!(body.contains("id=\"known-host-button\""));
    assert!(body.contains("data-signals:workflow.collect.mode=\"'upload'\""));
    assert!(body.contains("data-signals:workflow.collect.source=\"'upload-file'\""));
    assert!(body.contains("id=\"collect-save-toggle\""));
    assert!(body.contains("Download Archive"));
    assert!(body.contains("id=\"upload-form\""));
    assert!(body.contains(
        "data-attr:disabled=\"$workflow.collect.mode === 'upload' && $workflow.collect.source === 'upload-file'\""
    ));
    assert!(body.contains("placeholder=\"/"));

    server.shutdown().await;
}

#[tokio::test]
async fn workflow_page_shows_process_controls_and_forward_remote_input() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .get(format!("{base}/workflow"))
        .send()
        .await
        .expect("workflow page");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("workflow page body");

    assert!(body.contains("id=\"process-product\""));
    assert!(body.contains("id=\"process-type\""));
    assert!(body.contains("Diagnostic Processors"));
    assert!(body.contains("<option value=\"custom\">custom</option>"));
    assert!(body.contains("id=\"process-custom-processors\""));
    assert!(body.contains("id=\"process-enabled-toggle\""));
    assert!(body.contains("id=\"process-option-list\""));
    assert!(body.contains("id=\"process-selected-options\""));
    assert!(body.contains("id=\"send-forward-upload\""));
    assert!(body.contains("Forward preserves the raw diagnostic archive"));

    server.shutdown().await;
}

#[tokio::test]
async fn workflow_page_embeds_send_target_disable_and_auto_save_rules() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .get(format!("{base}/workflow"))
        .send()
        .await
        .expect("workflow page");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("workflow page body");

    assert!(body.contains("data-bind:workflow.process.enabled"));
    assert!(body.contains("$workflow.process.mode = $workflow.process.enabled ? 'process' : 'forward'"));
    assert!(body.contains("$workflow.send.mode = 'local'"));
    assert!(body.contains("Download the archive from&nbsp;<strong>Collect</strong>."));
    assert!(body.contains("$workflow.collect.source === 'known-host'"));
    assert!(body.contains("$workflow.collect.mode === 'collect' && $workflow.collect.source === 'known-host' && $workflow.collect.known_host !== ''"));
    assert!(body.contains("id=\"send-local-directory\""));
    assert!(body.contains("Local directory"));
    assert!(body.contains("id=\"workflow-go-button\""));
    assert!(body.contains("id=\"known-host-button\""));
    assert!(body.contains("<path d=\"M6 3L11 8L6 13\"></path>"));

    server.shutdown().await;
}

#[tokio::test]
async fn user_mode_workflow_exposes_browser_download_binding_and_local_directory_default() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .get(format!("{base}/workflow"))
        .send()
        .await
        .expect("workflow page");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("workflow page body");

    assert!(body.contains("data-signals:archive.download_token=\"''\""));
    assert!(body.contains("id=\"workflow-download-anchor\""));
    assert!(body.contains("data-on:change=\"if (evt.target.checked)"));
    assert!(body.contains("crypto.randomUUID()"));
    assert!(body.contains(
        "data-attr:href=\"$archive.download_token ? `/workflow/download/${$archive.download_token}` : null\""
    ));
    assert!(body.contains("/Downloads"));

    server.shutdown().await;
}
