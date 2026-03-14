use super::{
    ServerState, Signals, Tab, api_key, append_body_event, execute_script_event, file_upload,
    html_event, service_link, signal_event,
};
use crate::data::{KnownHost, Settings, authenticate, keystore_exists};
use crate::server::template::{
    self, KeystoreBootstrapModal, KeystoreProcessUnlockModal, KeystoreUnlockModal,
};
use askama::Template;
use axum::{
    extract::{Form, State},
    http::{HeaderMap, HeaderValue, header::RETRY_AFTER},
    response::{IntoResponse, Response},
};
use datastar::axum::ReadSignals;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize, Default)]
pub(crate) struct KeystoreForm {
    #[serde(default)]
    password: String,
    #[serde(default)]
    confirm: Option<String>,
}

fn resolve_request_user(state: &Arc<ServerState>, headers: &HeaderMap) -> String {
    state
        .resolve_user_email(headers)
        .map(|(_, user)| user)
        .unwrap_or_else(|_| "Anonymous".to_string())
}

pub async fn get_unlock_modal(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
) -> Response {
    if !keystore_exists().unwrap_or(false) {
        return get_bootstrap_modal(State(state), headers).await;
    }
    let modal = KeystoreUnlockModal {};
    match modal.render() {
        Ok(html) => state.publish_event(append_body_event(html)),
        Err(err) => state.publish_event(html_event(format!("<div>Error: {}</div>", err))),
    }
    axum::http::StatusCode::NO_CONTENT.into_response()
}

pub async fn get_process_unlock_modal(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
) -> Response {
    if !keystore_exists().unwrap_or(false) {
        return get_bootstrap_modal(State(state), headers).await;
    }

    let modal = KeystoreProcessUnlockModal {};
    match modal.render() {
        Ok(html) => state.publish_event(append_body_event(html)),
        Err(err) => state.publish_event(html_event(format!("<div>Error: {}</div>", err))),
    }
    axum::http::StatusCode::NO_CONTENT.into_response()
}

fn migration_needed() -> bool {
    if keystore_exists().unwrap_or(false) {
        return false;
    }

    match KnownHost::parse_hosts_yml() {
        Ok(hosts) => hosts.values().any(KnownHost::has_legacy_secret),
        Err(err) => {
            tracing::warn!(
                "Unable to inspect hosts.yml for plaintext secret migration: {}",
                err
            );
            false
        }
    }
}

fn missing_keystore_unlock_message() -> &'static str {
    if migration_needed() {
        "Migrate hosts to a new keystore before unlocking."
    } else {
        "Create a keystore before unlocking."
    }
}

async fn missing_keystore_response(state: &Arc<ServerState>, headers: HeaderMap) -> Response {
    state.publish_event(signal_event(format!(
        r#"{{"message":"{}"}}"#,
        missing_keystore_unlock_message()
    )));
    let _ = get_bootstrap_modal(State(state.clone()), headers).await;
    axum::http::StatusCode::PRECONDITION_FAILED.into_response()
}

async fn blocked_unlock_response(state: &Arc<ServerState>, user: &str) -> Option<Response> {
    let blocked_until = state.keystore_blocked_until_for(user).await?;
    let now = chrono::Utc::now().timestamp();
    if blocked_until <= now {
        return None;
    }

    let retry_after = blocked_until - now;
    state.publish_event(signal_event(format!(
        r#"{{"message":"Keystore temporarily locked. Retry in {} seconds."}}"#,
        retry_after
    )));
    let mut response = axum::http::StatusCode::TOO_MANY_REQUESTS.into_response();
    if let Ok(value) = HeaderValue::from_str(&retry_after.to_string()) {
        response.headers_mut().insert(RETRY_AFTER, value);
    }
    Some(response)
}

pub async fn get_bootstrap_modal(
    State(state): State<Arc<ServerState>>,
    _headers: HeaderMap,
) -> Response {
    if keystore_exists().unwrap_or(false) {
        return axum::http::StatusCode::NO_CONTENT.into_response();
    }

    let modal = KeystoreBootstrapModal {
        migrate: migration_needed(),
    };
    match modal.render() {
        Ok(html) => state.publish_event(append_body_event(html)),
        Err(err) => state.publish_event(html_event(format!("<div>Error: {}</div>", err))),
    }
    axum::http::StatusCode::NO_CONTENT.into_response()
}

pub async fn bootstrap(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Form(form): Form<KeystoreForm>,
) -> Response {
    if keystore_exists().unwrap_or(false) {
        return axum::http::StatusCode::NO_CONTENT.into_response();
    }

    if form.confirm.is_none() {
        state.publish_event(signal_event(
            r#"{"message":"Please confirm keystore creation to continue."}"#,
        ));
        state.publish_event(signal_event(r#"{"keystore":{"invalid":true}}"#));
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    }

    let password = form.password.trim().to_string();
    if password.len() < 6 {
        state.publish_event(signal_event(
            r#"{"message":"Keystore password must be at least 6 characters."}"#,
        ));
        state.publish_event(signal_event(
            r#"{"keystore":{"password":"","invalid":true}}"#,
        ));
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    }

    let should_migrate = migration_needed();

    if let Err(err) = authenticate(&password) {
        state.publish_event(signal_event(
            json!({ "message": format!("Failed to initialize keystore: {err}") }).to_string(),
        ));
        tracing::error!("Keystore bootstrap failed: {}", err);
        return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if should_migrate && let Err(err) = KnownHost::migrate_hosts_to_keystore(&password) {
        state.publish_event(signal_event(
            json!({ "message": format!("Failed to migrate hosts to keystore: {err}") }).to_string(),
        ));
        tracing::error!("Keystore migration failed: {}", err);
        return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let user = resolve_request_user(&state, &headers);
    state.set_keystore_unlocked_for(&user, password).await;
    state.publish_event(signal_event(
        r#"{"keystore":{"password":"","invalid":false,"confirm":false}}"#,
    ));
    state.publish_event(execute_script_event(
        "if (window.location.pathname === '/settings') { window.location.reload(); }",
    ));
    state.publish_event(html_event(
        r#"<div id="keystore-bootstrap-modal" data-init="document.getElementById('keystore-bootstrap-modal')?.remove();"></div>"#,
    ));
    axum::http::StatusCode::NO_CONTENT.into_response()
}

pub async fn unlock(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Form(form): Form<KeystoreForm>,
) -> Response {
    let user = resolve_request_user(&state, &headers);
    if !keystore_exists().unwrap_or(false) {
        return missing_keystore_response(&state, headers).await;
    }

    if let Some(response) = blocked_unlock_response(&state, &user).await {
        return response;
    }

    let password = form.password.trim().to_string();
    if password.is_empty() {
        state.publish_event(signal_event(
            r#"{"message":"Keystore password is required."}"#,
        ));
        state.publish_event(signal_event(
            r#"{"keystore":{"password":"","invalid":true}}"#,
        ));
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    }

    match authenticate(&password) {
        Ok(_) => {
            state.set_keystore_unlocked_for(&user, password).await;
            state.publish_event(signal_event(
                r#"{"keystore":{"password":"","invalid":false}}"#,
            ));
            state.publish_event(execute_script_event(
                "if (window.location.pathname === '/settings') { window.location.reload(); }",
            ));
            state.publish_event(html_event(
                r#"<div id="keystore-unlock-modal" data-init="document.getElementById('keystore-unlock-modal')?.remove();"></div>"#,
            ));
            axum::http::StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => {
            state.record_keystore_failed_attempt_for(&user).await;
            state.publish_event(signal_event(
                r#"{"message":"Invalid keystore password. Try again."}"#,
            ));
            state.publish_event(signal_event(
                r#"{"keystore":{"password":"","invalid":true}}"#,
            ));
            tracing::warn!("Keystore unlock failed: {}", err);
            axum::http::StatusCode::UNAUTHORIZED.into_response()
        }
    }
}

pub async fn unlock_and_run(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    ReadSignals(signals): ReadSignals<Signals>,
) -> Response {
    let user = resolve_request_user(&state, &headers);
    if !keystore_exists().unwrap_or(false) {
        return missing_keystore_response(&state, headers.clone()).await;
    }

    if let Some(response) = blocked_unlock_response(&state, &user).await {
        return response;
    }

    let password = signals.keystore.password.trim().to_string();
    if password.is_empty() {
        state.publish_event(signal_event(
            r#"{"message":"Keystore password is required."}"#,
        ));
        state.publish_event(signal_event(
            r#"{"keystore":{"password":"","invalid":true}}"#,
        ));
        return axum::http::StatusCode::NO_CONTENT.into_response();
    }

    match authenticate(&password) {
        Ok(_) => {
            state.set_keystore_unlocked_for(&user, password).await;
            state.publish_event(signal_event(
                r#"{"keystore":{"password":"","invalid":false}}"#,
            ));
            state.publish_event(html_event(
                r#"<div id="keystore-process-unlock-modal" data-init="document.getElementById('keystore-process-unlock-modal')?.remove();"></div>"#,
            ));

            match signals.tab {
                Tab::FileUpload => {
                    file_upload::process(State(state), headers, ReadSignals(signals))
                        .await
                        .into_response()
                }
                Tab::ServiceLink => service_link::form(State(state), headers, ReadSignals(signals))
                    .await
                    .into_response(),
                Tab::ApiKey => api_key::form(State(state), headers, ReadSignals(signals))
                    .await
                    .into_response(),
            }
        }
        Err(err) => {
            state.record_keystore_failed_attempt_for(&user).await;
            state.publish_event(signal_event(
                r#"{"message":"Invalid keystore password. Try again."}"#,
            ));
            state.publish_event(signal_event(
                r#"{"keystore":{"password":"","invalid":true}}"#,
            ));
            tracing::warn!("Keystore unlock-and-run failed: {}", err);
            axum::http::StatusCode::NO_CONTENT.into_response()
        }
    }
}

pub async fn lock(State(state): State<Arc<ServerState>>, headers: HeaderMap) -> impl IntoResponse {
    let user = resolve_request_user(&state, &headers);
    state.set_keystore_locked_for(&user, "manual").await;
    state.publish_event(execute_script_event(
        "if (window.location.pathname === '/settings') { window.location.reload(); }",
    ));
    axum::http::StatusCode::NO_CONTENT
}

pub async fn ensure_unlocked_for_active_output(
    state: &Arc<ServerState>,
    user: &str,
) -> Result<(), String> {
    let exporter = state.exporter.read().await.clone();
    if !cfg!(feature = "keystore") || !state.runtime_mode_policy.allows_local_artifacts() {
        // When the keystore is unavailable, the active exporter can still be valid if its
        // credentials were provided directly by the runtime environment instead of local
        // keystore-backed artifacts. In that mode there is nothing to unlock here.
        let _ = exporter;
        return Ok(());
    }

    let hosts_by_name = KnownHost::parse_hosts_yml().unwrap_or_default();
    let preferred_target = Settings::load()
        .ok()
        .and_then(|settings| settings.active_target);
    let (_output_options, selected_output, _exporter_label) = template::build_footer_output_context(
        &hosts_by_name,
        &exporter,
        preferred_target.as_deref(),
    );

    if !template::active_output_requires_keystore(&hosts_by_name, &selected_output, &exporter) {
        return Ok(());
    }

    if !state.is_keystore_unlocked_for(user).await {
        return Err("Keystore is locked. Unlock it before processing secure outputs.".to_string());
    }

    state.touch_keystore_session_for(user).await;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::{
        KeystoreForm, bootstrap, ensure_unlocked_for_active_output, get_process_unlock_modal,
        get_unlock_modal, lock, unlock, unlock_and_run,
    };
    use crate::{
        data::{KnownHost, Settings, authenticate},
        exporter::Exporter,
        server::{
            KeystoreSessionState, RuntimeMode, RuntimeModePolicy, ServerEvent, ServerState,
            Signals, Stats, Tab, test_server_state,
        },
    };
    use axum::{
        extract::{Form, State},
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
    };
    use bytes::Bytes;
    use datastar::axum::ReadSignals;
    use std::{
        collections::{BTreeMap, HashMap},
        path::PathBuf,
        sync::{Arc, Mutex},
    };
    use tempfile::TempDir;
    use tokio::sync::{RwLock, broadcast, watch};
    use url::Url;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_env_lock()
    }

    fn setup_env() -> (TempDir, PathBuf, PathBuf) {
        let tmp = TempDir::new().expect("temp dir");
        let config_dir = tmp.path().join(".esdiag");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        let hosts_path = config_dir.join("hosts.yml");
        let keystore_path = config_dir.join("secrets.yml");
        unsafe {
            std::env::set_var("HOME", tmp.path());
            std::env::set_var("USERPROFILE", tmp.path());
            std::env::set_var("ESDIAG_HOSTS", &hosts_path);
            std::env::set_var("ESDIAG_KEYSTORE", &keystore_path);
        }
        (tmp, hosts_path, keystore_path)
    }

    fn write_hosts(hosts: BTreeMap<String, KnownHost>) {
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");
    }

    fn test_service_state() -> Arc<ServerState> {
        let (stats_updates_tx, stats_updates_rx) = watch::channel(0u64);
        let runtime_mode = RuntimeMode::Service;
        Arc::new(ServerState {
            exporter: Arc::new(RwLock::new(Exporter::default())),
            kibana_url: Arc::new(RwLock::new(String::new())),
            signals: Arc::new(RwLock::new(Signals::default())),
            uploads: Arc::new(RwLock::new(HashMap::<u64, (String, Bytes)>::new())),
            links: Arc::new(RwLock::new(HashMap::new())),
            keys: Arc::new(RwLock::new(HashMap::new())),
            runtime_mode,
            runtime_mode_policy: RuntimeModePolicy::new(runtime_mode),
            keystore_state: Arc::new(RwLock::new(KeystoreSessionState::default())),
            stats: Arc::new(RwLock::new(Stats::default())),
            shutdown: watch::channel(false).1,
            event_tx: broadcast::channel(16).0,
            stats_updates_tx,
            stats_updates_rx,
        })
    }

    #[tokio::test]
    async fn unlock_is_idempotent_and_emits_signal_updates() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, keystore_path) = setup_env();
        authenticate("pw").expect("create keystore");
        assert!(keystore_path.is_file(), "keystore should exist");

        let state = test_server_state();
        let mut events = state.subscribe_events();

        let response = unlock(
            State(state.clone()),
            HeaderMap::new(),
            Form(KeystoreForm {
                password: "pw".to_string(),
                confirm: None,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            !state.keystore_status().await.0,
            "unlock should transition state to unlocked"
        );

        let first = events.recv().await.expect("unlock signal");
        match first {
            ServerEvent::Signals(payload) => {
                assert!(payload.contains(r#""keystore":{"locked":false"#));
            }
            other => panic!("expected keystore signal, got {other:?}"),
        }

        let response = unlock(
            State(state.clone()),
            HeaderMap::new(),
            Form(KeystoreForm {
                password: "pw".to_string(),
                confirm: None,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            !state.keystore_status().await.0,
            "repeat unlock should remain unlocked"
        );
    }

    #[tokio::test]
    async fn unlock_rejects_invalid_password_with_401_and_stays_locked() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();
        authenticate("pw").expect("create keystore");

        let state = test_server_state();
        let response = unlock(
            State(state.clone()),
            HeaderMap::new(),
            Form(KeystoreForm {
                password: "wrong".to_string(),
                confirm: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(
            state.keystore_status().await.0,
            "state should remain locked"
        );
    }

    #[tokio::test]
    async fn unlock_requires_existing_keystore() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, keystore_path) = setup_env();
        assert!(!keystore_path.exists(), "keystore should start missing");

        let state = test_server_state();
        let response = unlock(
            State(state),
            HeaderMap::new(),
            Form(KeystoreForm {
                password: "pw".to_string(),
                confirm: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);
    }

    #[tokio::test]
    async fn unlock_missing_keystore_with_plaintext_hosts_prompts_migration() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, keystore_path) = setup_env();
        assert!(!keystore_path.exists(), "keystore should start missing");

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "legacy".to_string(),
            KnownHost::ApiKey {
                accept_invalid_certs: false,
                apikey: Some("plaintext-api-key".to_string()),
                app: crate::data::Product::Elasticsearch,
                cloud_id: None,
                roles: vec![crate::data::HostRole::Send],
                secret: None,
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        write_hosts(hosts);

        let state = test_server_state();
        let mut events = state.subscribe_events();
        let response = unlock(
            State(state),
            HeaderMap::new(),
            Form(KeystoreForm {
                password: "pw".to_string(),
                confirm: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);

        let first = events.recv().await.expect("migration message event");
        match first {
            ServerEvent::Signals(payload) => {
                assert!(payload.contains("Migrate hosts to a new keystore before unlocking."));
            }
            other => panic!("expected migration message signal, got {other:?}"),
        }

        let second = events.recv().await.expect("bootstrap modal event");
        match second {
            ServerEvent::AppendBody(html) => {
                assert!(html.contains("keystore-bootstrap-modal"));
                assert!(html.contains("Migrate to Keystore"));
            }
            other => panic!("expected bootstrap modal append event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn missing_keystore_unlock_modal_falls_back_to_bootstrap_modal() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, keystore_path) = setup_env();
        assert!(!keystore_path.exists(), "keystore should start missing");

        let state = test_server_state();
        let mut events = state.subscribe_events();
        let response = get_unlock_modal(State(state), HeaderMap::new()).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let event = events.recv().await.expect("bootstrap modal event");
        match event {
            ServerEvent::AppendBody(html) => {
                assert!(html.contains("keystore-bootstrap-modal"));
                assert!(html.contains("Create Keystore"));
            }
            other => panic!("expected bootstrap modal append event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn missing_keystore_process_unlock_modal_falls_back_to_bootstrap_modal() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, keystore_path) = setup_env();
        assert!(!keystore_path.exists(), "keystore should start missing");

        let state = test_server_state();
        let mut events = state.subscribe_events();
        let response = get_process_unlock_modal(State(state), HeaderMap::new()).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let event = events.recv().await.expect("bootstrap modal event");
        match event {
            ServerEvent::AppendBody(html) => {
                assert!(html.contains("keystore-bootstrap-modal"));
                assert!(html.contains("Create Keystore"));
            }
            other => panic!("expected bootstrap modal append event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn bootstrap_modal_uses_create_prompt_when_hosts_have_no_plaintext_secrets() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, hosts_path, keystore_path) = setup_env();
        assert!(!keystore_path.exists(), "keystore should start missing");
        assert!(!hosts_path.exists(), "hosts file should start missing");

        let state = test_server_state();
        let mut events = state.subscribe_events();
        let response = get_unlock_modal(State(state), HeaderMap::new()).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            hosts_path.is_file(),
            "hosts file should be created when inspected"
        );

        let event = events.recv().await.expect("bootstrap modal event");
        match event {
            ServerEvent::AppendBody(html) => {
                assert!(html.contains("keystore-bootstrap-modal"));
                assert!(html.contains("Create Keystore"));
                assert!(!html.contains("Migrate to Keystore"));
            }
            other => panic!("expected bootstrap modal append event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn bootstrap_modal_uses_migrate_prompt_only_when_plaintext_secrets_exist() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, keystore_path) = setup_env();
        assert!(!keystore_path.exists(), "keystore should start missing");

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "legacy".to_string(),
            KnownHost::ApiKey {
                accept_invalid_certs: false,
                apikey: Some("plaintext-api-key".to_string()),
                app: crate::data::Product::Elasticsearch,
                cloud_id: None,
                roles: vec![crate::data::HostRole::Send],
                secret: None,
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        write_hosts(hosts);

        let state = test_server_state();
        let mut events = state.subscribe_events();
        let response = get_unlock_modal(State(state), HeaderMap::new()).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let event = events.recv().await.expect("bootstrap modal event");
        match event {
            ServerEvent::AppendBody(html) => {
                assert!(html.contains("keystore-bootstrap-modal"));
                assert!(html.contains("Migrate to Keystore"));
            }
            other => panic!("expected bootstrap modal append event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn bootstrap_migrates_plaintext_hosts_into_new_keystore() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, keystore_path) = setup_env();
        assert!(!keystore_path.exists(), "keystore should start missing");

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "legacy-es".to_string(),
            KnownHost::ApiKey {
                accept_invalid_certs: false,
                apikey: Some("plaintext-api-key".to_string()),
                app: crate::data::Product::Elasticsearch,
                cloud_id: None,
                roles: vec![crate::data::HostRole::Send],
                secret: None,
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        write_hosts(hosts);

        let state = test_server_state();
        let response = bootstrap(
            State(state),
            HeaderMap::new(),
            Form(KeystoreForm {
                password: "secretpw".to_string(),
                confirm: Some("on".to_string()),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(keystore_path.is_file(), "keystore should be created");

        let migrated_hosts = KnownHost::parse_hosts_yml().expect("reload migrated hosts");
        match migrated_hosts.get("legacy-es").expect("migrated host") {
            KnownHost::ApiKey { apikey, secret, .. } => {
                assert!(apikey.is_none(), "plaintext apikey should be scrubbed");
                assert_eq!(secret.as_deref(), Some("legacy-es"));
            }
            _ => panic!("expected migrated api key host"),
        }
    }

    #[tokio::test]
    async fn lock_is_idempotent_and_preserves_locked_state() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();
        let state = test_server_state();
        state.set_keystore_unlocked("pw".to_string()).await;

        let response = lock(State(state.clone()), HeaderMap::new())
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(state.keystore_status().await.0, "lock should relock state");

        let response = lock(State(state.clone()), HeaderMap::new())
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(state.keystore_status().await.0, "repeat lock stays locked");
    }

    #[tokio::test]
    async fn unlock_initializes_and_refreshes_session_lease() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();
        authenticate("pw").expect("create keystore");

        let state = test_server_state();
        unlock(
            State(state.clone()),
            HeaderMap::new(),
            Form(KeystoreForm {
                password: "pw".to_string(),
                confirm: None,
            }),
        )
        .await;

        let first_expiry = state
            .keystore_expires_at_epoch()
            .await
            .expect("expiry initialized");
        let first_lock_time = state.keystore_status().await.1;
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        state.touch_keystore_session().await;

        let refreshed_expiry = state
            .keystore_expires_at_epoch()
            .await
            .expect("expiry refreshed");
        let refreshed_lock_time = state.keystore_status().await.1;
        assert!(refreshed_expiry >= first_expiry);
        assert_eq!(refreshed_lock_time, first_lock_time);
    }

    #[tokio::test]
    async fn secure_output_requests_refresh_session_and_noauth_bypasses_unlock() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();

        let noauth_host = KnownHost::NoAuth {
            app: crate::data::Product::Elasticsearch,
            roles: vec![crate::data::HostRole::Send],
            viewer: None,
            url: Url::parse("http://localhost:9200").expect("url"),
        };
        let secure_host = KnownHost::Basic {
            accept_invalid_certs: false,
            app: crate::data::Product::Elasticsearch,
            password: None,
            roles: vec![crate::data::HostRole::Send],
            secret: Some("secure".to_string()),
            viewer: None,
            url: Url::parse("https://secure.example.com:9200").expect("url"),
            username: None,
        };

        let mut hosts = BTreeMap::new();
        hosts.insert("noauth".to_string(), noauth_host.clone());
        hosts.insert("secure".to_string(), secure_host.clone());
        write_hosts(hosts);

        let mut settings = Settings {
            active_target: Some("noauth".to_string()),
            ..Settings::default()
        };
        settings.save().expect("save settings");

        let state = test_server_state();
        *state.exporter.write().await =
            crate::exporter::Exporter::try_from(noauth_host).expect("noauth exporter");
        assert!(
            ensure_unlocked_for_active_output(&state, "Anonymous")
                .await
                .is_ok(),
            "NoAuth output should bypass keystore preflight"
        );

        settings.active_target = Some("secure".to_string());
        settings.save().expect("save secure settings");
        *state.exporter.write().await = crate::exporter::Exporter::try_from(KnownHost::NoAuth {
            app: crate::data::Product::Elasticsearch,
            roles: vec![crate::data::HostRole::Send],
            viewer: None,
            url: Url::parse("https://secure.example.com:9200").expect("secure url"),
        })
        .expect("secure exporter");
        assert!(
            ensure_unlocked_for_active_output(&state, "Anonymous")
                .await
                .is_err(),
            "secure output should require unlock"
        );

        state.set_keystore_unlocked("pw".to_string()).await;
        let first_lock_time = state.keystore_status().await.1;
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        ensure_unlocked_for_active_output(&state, "Anonymous")
            .await
            .expect("secure output should pass once unlocked");
        let refreshed_lock_time = state.keystore_status().await.1;
        assert_eq!(refreshed_lock_time, first_lock_time);
    }

    #[tokio::test]
    async fn service_mode_non_secure_output_bypasses_keystore_preflight() {
        let state = test_service_state();
        *state.exporter.write().await = crate::exporter::Exporter::try_from(KnownHost::NoAuth {
            app: crate::data::Product::Elasticsearch,
            roles: vec![crate::data::HostRole::Send],
            viewer: None,
            url: Url::parse("http://localhost:9200").expect("url"),
        })
        .expect("noauth exporter");

        assert!(
            ensure_unlocked_for_active_output(&state, "Anonymous")
                .await
                .is_ok(),
            "service mode should allow non-secure outputs without keystore access"
        );
    }

    #[tokio::test]
    async fn service_mode_secure_output_bypasses_unlock_and_does_not_touch_local_artifacts() {
        let _guard = env_lock().lock().expect("env lock");
        let (tmp, hosts_path, _keystore_path) = setup_env();
        let settings_path = tmp.path().join(".esdiag").join("settings.yml");
        unsafe {
            std::env::set_var("ESDIAG_SETTINGS", &settings_path);
        }

        let state = test_service_state();
        *state.exporter.write().await = crate::exporter::Exporter::try_from(KnownHost::ApiKey {
            accept_invalid_certs: false,
            apikey: Some("secret".to_string()),
            app: crate::data::Product::Elasticsearch,
            cloud_id: None,
            roles: vec![crate::data::HostRole::Send],
            secret: None,
            viewer: None,
            url: Url::parse("https://secure.example.com:9200").expect("url"),
        })
        .expect("secure exporter");

        let result = ensure_unlocked_for_active_output(&state, "Anonymous").await;

        assert!(
            result.is_ok(),
            "service mode should allow preconfigured secure outputs without keystore unlock"
        );
        assert!(
            !hosts_path.exists(),
            "service mode preflight should not touch hosts.yml"
        );
        assert!(
            !settings_path.exists(),
            "service mode preflight should not touch settings.yml"
        );
    }

    #[tokio::test]
    async fn failed_unlock_backoff_starts_on_fourth_failure_and_caps_at_sixty_minutes() {
        let state = test_server_state();

        for _ in 0..3 {
            assert_eq!(state.record_keystore_failed_attempt().await, None);
        }

        let fourth = state
            .record_keystore_failed_attempt()
            .await
            .expect("fourth failure should set backoff");
        let now = chrono::Utc::now().timestamp();
        assert!((fourth - now) >= 299 && (fourth - now) <= 300);

        for _ in 0..11 {
            state.record_keystore_failed_attempt().await;
        }
        let capped = state
            .record_keystore_failed_attempt()
            .await
            .expect("backoff should remain capped");
        let now = chrono::Utc::now().timestamp();
        assert!((capped - now) >= 3599 && (capped - now) <= 3600);
        assert!(state.keystore_failed_attempts().await >= 12);
    }

    #[tokio::test]
    async fn unlock_and_run_success_unlocks_and_resumes_processing() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();
        authenticate("pw").expect("create keystore");

        let state = test_server_state();
        let mut signals = Signals {
            tab: Tab::FileUpload,
            ..Signals::default()
        };
        signals.keystore.password = "pw".to_string();

        let response =
            unlock_and_run(State(state.clone()), HeaderMap::new(), ReadSignals(signals)).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            !state.keystore_status().await.0,
            "successful preflight should unlock the keystore"
        );
    }

    #[tokio::test]
    async fn unlock_and_run_failure_keeps_processing_blocked_and_marks_invalid() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();
        authenticate("pw").expect("create keystore");

        let state = test_server_state();
        let mut events = state.subscribe_events();
        let mut signals = Signals {
            tab: Tab::ApiKey,
            ..Signals::default()
        };
        signals.keystore.password = "wrong".to_string();

        let response =
            unlock_and_run(State(state.clone()), HeaderMap::new(), ReadSignals(signals)).await;

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            state.keystore_status().await.0,
            "state should remain locked"
        );

        let mut saw_invalid = false;
        while let Ok(event) = events.try_recv() {
            if let ServerEvent::Signals(payload) = event
                && payload.contains(r#""keystore":{"password":"","invalid":true}"#)
            {
                saw_invalid = true;
            }
        }
        assert!(saw_invalid, "expected invalid-password keystore signal");
    }

    #[tokio::test]
    async fn unlock_empty_password_marks_field_invalid() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();
        authenticate("pw").expect("create keystore");

        let state = test_server_state();
        let mut events = state.subscribe_events();
        let response = unlock(
            State(state),
            HeaderMap::new(),
            Form(KeystoreForm {
                password: "   ".to_string(),
                confirm: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let mut saw_invalid = false;
        while let Ok(event) = events.try_recv() {
            if let ServerEvent::Signals(payload) = event
                && payload.contains(r#""keystore":{"password":"","invalid":true}"#)
            {
                saw_invalid = true;
            }
        }
        assert!(saw_invalid, "expected empty-password keystore signal");
    }

    #[tokio::test]
    async fn unlock_and_run_requires_existing_keystore_before_processing() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, keystore_path) = setup_env();
        assert!(!keystore_path.exists(), "keystore should start missing");

        let state = test_server_state();
        let mut events = state.subscribe_events();
        let mut signals = Signals {
            tab: Tab::FileUpload,
            ..Signals::default()
        };
        signals.keystore.password = "pw".to_string();

        let response = unlock_and_run(State(state), HeaderMap::new(), ReadSignals(signals)).await;

        assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);

        let first = events.recv().await.expect("missing keystore message event");
        match first {
            ServerEvent::Signals(payload) => {
                assert!(payload.contains("Create a keystore before unlocking."));
            }
            other => panic!("expected missing keystore signal, got {other:?}"),
        }

        let second = events.recv().await.expect("bootstrap modal event");
        match second {
            ServerEvent::AppendBody(html) => {
                assert!(html.contains("keystore-bootstrap-modal"));
                assert!(html.contains("Create Keystore"));
            }
            other => panic!("expected bootstrap modal append event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unlock_and_run_respects_backoff_and_retry_after() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();
        authenticate("pw").expect("create keystore");

        let state = test_server_state();
        for _ in 0..4 {
            state.record_keystore_failed_attempt().await;
        }

        let mut signals = Signals {
            tab: Tab::FileUpload,
            ..Signals::default()
        };
        signals.keystore.password = "pw".to_string();

        let response = unlock_and_run(State(state), HeaderMap::new(), ReadSignals(signals)).await;

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(
            response
                .headers()
                .contains_key(axum::http::header::RETRY_AFTER),
            "blocked unlock-and-run should include Retry-After"
        );
    }
}
