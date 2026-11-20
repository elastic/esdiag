#![cfg(feature = "server")]

use esdiag::{
    exporter::Exporter,
    server::{AuthProvider, RuntimeMode, Server, ServerStartOptions},
};
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;

const API_KEY_TAB_ID: &str = "tab-api-key";
const API_KEY_FORM_ID: &str = "api-key-form";

/// True when an element in `html` carries the attribute `id="{id}"`.
fn has_element_id(html: &str, id: &str) -> bool {
    let needle = format!("id=\"{id}\"");
    html.match_indices(&needle)
        .any(|(at, _)| html[..at].ends_with(char::is_whitespace))
}

/// Link targets of the primary navigation, or `None` when the page renders no navigation.
fn nav_links(html: &str) -> Option<Vec<&str>> {
    let start = html.find("<header-nav>")? + "<header-nav>".len();
    let end = start + html[start..].find("</header-nav>")?;
    Some(
        html[start..end]
            .split("href=\"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .collect(),
    )
}

async fn start_server(mode: RuntimeMode) -> (Server, Client, String) {
    let default_web_features = match mode {
        RuntimeMode::User => Some("advanced"),
        RuntimeMode::Service => Some(""),
    };
    start_server_with_features(mode, default_web_features).await
}

async fn start_server_with_features(mode: RuntimeMode, web_features: Option<&str>) -> (Server, Client, String) {
    start_server_with_options(
        mode,
        ServerStartOptions {
            web_features,
            ..ServerStartOptions::default()
        },
    )
    .await
}

async fn start_server_with_options(mode: RuntimeMode, options: ServerStartOptions<'_>) -> (Server, Client, String) {
    let (server, bound_addr) =
        Server::start_with_options([127, 0, 0, 1], 0, Exporter::default(), String::new(), mode, options)
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
async fn service_mode_reads_user_from_configured_identity_header() {
    let (mut server, client, base) = start_server_with_options(
        RuntimeMode::Service,
        ServerStartOptions {
            auth_provider: Some(AuthProvider::GoogleIap),
            identity_header: Some("X-Authenticated-User"),
            web_features: Some(""),
            ..ServerStartOptions::default()
        },
    )
    .await;

    let default_header = client
        .get(format!("{base}/"))
        .header(
            "X-Goog-Authenticated-User-Email",
            "accounts.google.com:wrong@example.com",
        )
        .send()
        .await
        .expect("default header request");
    assert_eq!(default_header.status(), reqwest::StatusCode::UNAUTHORIZED);

    let configured_header = client
        .get(format!("{base}/"))
        .header("X-Authenticated-User", "ops@example.com")
        .send()
        .await
        .expect("configured header request");
    assert_eq!(configured_header.status(), reqwest::StatusCode::OK);
    assert!(
        configured_header
            .text()
            .await
            .expect("configured header body")
            .contains("ops@example.com")
    );

    server.shutdown().await;
}

#[tokio::test]
async fn service_mode_none_auth_accepts_requests_without_identity_header() {
    let (mut server, client, base) = start_server_with_options(
        RuntimeMode::Service,
        ServerStartOptions {
            auth_provider: Some(AuthProvider::None),
            web_features: Some(""),
            ..ServerStartOptions::default()
        },
    )
    .await;

    let response = client
        .get(format!("{base}/"))
        .send()
        .await
        .expect("local service request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(response.text().await.expect("local service body").contains("Anonymous"));

    server.shutdown().await;
}

#[tokio::test]
async fn service_mode_does_not_mount_advanced_or_jobs_routes() {
    let (mut server, client, base) = start_server(RuntimeMode::Service).await;

    let advanced_response = client
        .get(format!("{base}/advanced"))
        .header("X-Goog-Authenticated-User-Email", "accounts.google.com:ops@example.com")
        .send()
        .await
        .expect("service mode advanced request");
    assert_eq!(advanced_response.status(), reqwest::StatusCode::NOT_FOUND);

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
async fn service_mode_omits_api_key_ui_and_routes() {
    let (mut server, client, base) = start_server(RuntimeMode::Service).await;
    let identity = ("X-Goog-Authenticated-User-Email", "accounts.google.com:ops@example.com");

    let response = client
        .get(format!("{base}/"))
        .header(identity.0, identity.1)
        .send()
        .await
        .expect("service mode page");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("service mode body");
    // `user_mode_allows_anonymous_web_access` proves user mode renders both ids.
    assert!(!has_element_id(&body, API_KEY_TAB_ID));
    assert!(!has_element_id(&body, API_KEY_FORM_ID));

    for path in ["/api/api_key", "/api_key", "/api_key/42"] {
        let response = client
            .post(format!("{base}{path}"))
            .header(identity.0, identity.1)
            .json(&serde_json::json!({
                "apikey": "must-not-be-accepted",
                "url": "https://elasticsearch.example.com",
                "metadata": {}
            }))
            .send()
            .await
            .expect("service mode API key request");
        assert_eq!(
            response.status(),
            reqwest::StatusCode::NOT_FOUND,
            "service mode mounted {path}"
        );
    }

    server.shutdown().await;
}

#[tokio::test]
async fn user_mode_default_nav_exposes_advanced_not_job_builder() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client.get(format!("{base}/")).send().await.expect("user mode request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("user mode body");

    assert_eq!(nav_links(&body), Some(vec!["/", "/advanced"]));

    let advanced = client
        .get(format!("{base}/advanced"))
        .send()
        .await
        .expect("advanced route");
    assert_eq!(advanced.status(), reqwest::StatusCode::OK);

    let workflow = client
        .get(format!("{base}/workflow"))
        .send()
        .await
        .expect("workflow route");
    assert_eq!(workflow.status(), reqwest::StatusCode::NOT_FOUND);

    let jobs = client.get(format!("{base}/jobs")).send().await.expect("jobs route");
    assert_eq!(jobs.status(), reqwest::StatusCode::NOT_FOUND);

    server.shutdown().await;
}

#[cfg(feature = "keystore")]
#[tokio::test]
async fn explicit_job_builder_feature_mounts_jobs_and_omits_advanced() {
    let (mut server, client, base) = start_server_with_features(RuntimeMode::User, Some("job-builder")).await;

    let response = client.get(format!("{base}/")).send().await.expect("user mode request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("user mode body");
    assert_eq!(nav_links(&body), Some(vec!["/", "/jobs"]));

    let advanced = client
        .get(format!("{base}/advanced"))
        .send()
        .await
        .expect("advanced route");
    assert_eq!(advanced.status(), reqwest::StatusCode::NOT_FOUND);

    let jobs = client.get(format!("{base}/jobs")).send().await.expect("jobs route");
    assert_eq!(jobs.status(), reqwest::StatusCode::OK);

    server.shutdown().await;
}

#[cfg(feature = "keystore")]
#[tokio::test]
async fn explicit_advanced_and_job_builder_features_mount_both_pages() {
    let (mut server, client, base) = start_server_with_features(RuntimeMode::User, Some("advanced,job-builder")).await;

    let response = client.get(format!("{base}/")).send().await.expect("user mode request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("user mode body");
    assert_eq!(nav_links(&body), Some(vec!["/", "/advanced", "/jobs"]));

    let advanced = client
        .get(format!("{base}/advanced"))
        .send()
        .await
        .expect("advanced route");
    assert_eq!(advanced.status(), reqwest::StatusCode::OK);

    let jobs = client.get(format!("{base}/jobs")).send().await.expect("jobs route");
    assert_eq!(jobs.status(), reqwest::StatusCode::OK);

    server.shutdown().await;
}

#[tokio::test]
async fn empty_web_features_omit_optional_nav_and_routes() {
    let (mut server, client, base) = start_server_with_features(RuntimeMode::User, Some("")).await;

    let response = client.get(format!("{base}/")).send().await.expect("user mode request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("user mode body");
    // The other nav tests prove `nav_links` finds the navigation when it is rendered.
    assert_eq!(nav_links(&body), None);

    let advanced = client
        .get(format!("{base}/advanced"))
        .send()
        .await
        .expect("advanced route");
    assert_eq!(advanced.status(), reqwest::StatusCode::NOT_FOUND);

    let jobs = client.get(format!("{base}/jobs")).send().await.expect("jobs route");
    assert_eq!(jobs.status(), reqwest::StatusCode::NOT_FOUND);

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
    assert!(has_element_id(&body, "main-content"));
    assert!(has_element_id(&body, API_KEY_TAB_ID));
    assert!(has_element_id(&body, API_KEY_FORM_ID));

    server.shutdown().await;
}

#[tokio::test]
async fn user_mode_advanced_page_exposes_collect_process_and_send_controls() {
    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .get(format!("{base}/advanced"))
        .send()
        .await
        .expect("advanced page");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("advanced page body");

    for id in [
        // Collect stage: known-host collection is user-mode only; the other sources and archive download.
        "collect-known-host",
        "known-host-button",
        API_KEY_FORM_ID,
        "service-link-form",
        "upload-form",
        "collect-save-toggle",
        // Process stage.
        "process-enabled-toggle",
        "process-product",
        "process-type",
        "process-custom-processors",
        "process-option-list",
        "process-selected-options",
        // Send stage.
        "send-remote-target",
        "send-forward-upload",
        "send-local-target",
        "send-local-directory",
        // Run and browser download.
        "job-go-button",
        "job-download-anchor",
    ] {
        assert!(has_element_id(&body, id), "advanced page is missing #{id}");
    }

    server.shutdown().await;
}
