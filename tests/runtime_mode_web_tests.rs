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
async fn service_mode_index_does_not_render_process_unlock_routes() {
    let (mut server, client, base) = start_server(RuntimeMode::Service).await;

    let response = client
        .get(format!("{base}/"))
        .header(
            "X-Goog-Authenticated-User-Email",
            "accounts.google.com:ops@example.com",
        )
        .send()
        .await
        .expect("service mode request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("service mode body");
    assert!(
        !body.contains("/keystore/modal/process"),
        "service mode should not render process unlock modal routes"
    );

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
        .header(
            "X-Goog-Authenticated-User-Email",
            "accounts.google.com:ops@example.com",
        )
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
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    let response = client
        .post(format!("{base}/keystore/unlock"))
        .header(
            "X-Goog-Authenticated-User-Email",
            "accounts.google.com:ops@example.com",
        )
        .form(&[("password", "pw")])
        .send()
        .await
        .expect("service mode keystore request");
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    let response = client
        .get(format!("{base}/keystore/modal"))
        .header(
            "X-Goog-Authenticated-User-Email",
            "accounts.google.com:ops@example.com",
        )
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

#[tokio::test]
async fn user_mode_index_shows_known_host_collect_and_local_save_defaults() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .get(format!("{base}/"))
        .send()
        .await
        .expect("user mode request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("user mode body");

    assert!(body.contains("id=\"collect-known-host\""));
    assert!(body.contains("data-signals:workflow.collect.mode=\"'upload'\""));
    assert!(body.contains("data-signals:workflow.collect.source=\"'upload-file'\""));
    assert!(body.contains("id=\"collect-save-toggle\""));
    assert!(body.contains("Save Archive"));
    assert!(body.contains("id=\"collect-save-dir\""));
    assert!(body.contains("id=\"upload-form\""));
    assert!(body.contains(
        "data-attr:disabled=\"($workflow.collect.mode === 'upload' && $workflow.collect.source === 'upload-file')"
    ));
    assert!(body.contains("placeholder=\"/"));

    server.shutdown().await;
}

#[tokio::test]
async fn service_mode_index_hides_known_host_selection_and_disables_save() {
    let (mut server, client, base) = start_server(RuntimeMode::Service).await;

    let response = client
        .get(format!("{base}/"))
        .header("X-Goog-Authenticated-User-Email", "accounts.google.com:ops@example.com")
        .send()
        .await
        .expect("service mode authorized request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("service mode body");

    assert!(body.contains("data-signals:workflow.collect.mode=\"'upload'\""));
    assert!(body.contains("data-signals:workflow.collect.source=\"'upload-file'\""));
    assert!(body.contains("id=\"collect-save-toggle\""));
    assert!(body.contains("id=\"upload-form\""));
    assert!(body.contains("id=\"collect-save-toggle\""));
    assert!(body.contains("data-attr:disabled=\"($workflow.collect.mode === 'upload' && $workflow.collect.source === 'upload-file') || true\""));

    server.shutdown().await;
}

#[tokio::test]
async fn index_shows_process_controls_and_forward_remote_input() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .get(format!("{base}/"))
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
async fn index_embeds_send_target_disable_and_auto_save_rules() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .get(format!("{base}/"))
        .send()
        .await
        .expect("workflow page");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("workflow page body");

    assert!(body.contains("data-bind:workflow.process.enabled"));
    assert!(body.contains("$workflow.process.mode = $workflow.process.enabled ? 'process' : 'forward'"));
    assert!(body.contains("data-attr:disabled=\"!$workflow.process.enabled\""));
    assert!(body.contains("Local bundle retention is handled in `Collect` via Save Archive."));
    assert!(body.contains("Forward + Local` is disabled. Save the bundle in `Collect` to keep a local archive."));
    assert!(body.contains("id=\"send-local-directory\""));
    assert!(body.contains("Local directory"));
    assert!(body.contains("id=\"workflow-go-button\""));
    assert!(body.contains("collect + process + send"));

    server.shutdown().await;
}

#[tokio::test]
async fn user_mode_index_exposes_os_aware_save_dir_and_override_binding() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .get(format!("{base}/"))
        .send()
        .await
        .expect("workflow page");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("workflow page body");

    assert!(body.contains("data-signals:workflow.collect.save_dir=\"'"));
    assert!(body.contains("/Downloads"));
    assert!(body.contains("id=\"collect-save-dir\""));
    assert!(body.contains("data-bind:workflow.collect.save_dir"));

    server.shutdown().await;
}
