use super::{ServerState, append_body_event, execute_script_event, html_event, now_epoch_seconds, signal_event};
use crate::data::{HostRole, KnownHost, Settings, authenticate, keystore_exists};
use crate::server::template::{self, KeystoreBootstrapModal, KeystoreProcessUnlockModal, KeystoreUnlockModal};
use askama::Template;
use axum::{
    extract::{Form, State},
    http::{HeaderValue, header::RETRY_AFTER},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct KeystoreRateLimit {
    pub failed_attempts: u32,
    pub blocked_until_epoch: Option<i64>,
    last_lock_transition_epoch: Option<i64>,
    /// Tracks the last observed unlock state so we can detect transitions
    /// (e.g. lease expiry or CLI lock) and publish SSE signals to the browser.
    last_seen_unlocked: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct KeystorePageState {
    pub can_use_keystore: bool,
    pub locked: bool,
    pub lock_time: i64,
    pub show_bootstrap: bool,
}

impl KeystoreRateLimit {
    fn current_backoff_seconds(&self) -> u64 {
        if self.failed_attempts <= 3 {
            return 0;
        }
        let over = self.failed_attempts - 3;
        let minutes = (over as u64).saturating_mul(5).min(60);
        minutes * 60
    }
}

impl ServerState {
    pub(crate) async fn keystore_page_state(&self) -> KeystorePageState {
        #[cfg(feature = "keystore")]
        {
            if !self.can_use_keystore_session() {
                return KeystorePageState::default();
            }

            let (locked, lock_time) = self.keystore_status().await;
            KeystorePageState {
                can_use_keystore: true,
                locked,
                lock_time,
                show_bootstrap: !keystore_exists().unwrap_or(false),
            }
        }
        #[cfg(not(feature = "keystore"))]
        {
            KeystorePageState::default()
        }
    }

    fn observe_keystore_lock_state(&self) -> (bool, i64, bool) {
        let unlocked = matches!(crate::data::read_unlock_lease(), Ok(Some(_)));
        let now = now_epoch_seconds();
        let mut rate_limit = self.keystore_rate_limit.lock().expect("rate limit lock");
        let previous = rate_limit.last_seen_unlocked;
        let transitioned_to_locked = previous == Some(true) && !unlocked;
        if previous != Some(unlocked) {
            rate_limit.last_seen_unlocked = Some(unlocked);
            rate_limit.last_lock_transition_epoch = Some(now);
        }
        let lock_time = *rate_limit.last_lock_transition_epoch.get_or_insert(now);
        (!unlocked, lock_time, transitioned_to_locked)
    }

    fn render_keystore_signal_payload(locked: bool, lock_time: i64) -> String {
        format!(r#"{{"keystore":{{"locked":{},"lock_time":{}}}}}"#, locked, lock_time)
    }

    pub(crate) fn can_use_keystore_session(&self) -> bool {
        self.runtime_mode_policy.allows_local_runtime_features() && !self.runtime_mode_policy.requires_iap_headers()
    }

    pub async fn keystore_status(&self) -> (bool, i64) {
        if !self.can_use_keystore_session() {
            return (true, 0);
        }
        let (locked, lock_time, transitioned_to_locked) = self.observe_keystore_lock_state();
        if transitioned_to_locked {
            tracing::info!("Keystore lease expired or was cleared externally");
            self.publish_event(signal_event(Self::render_keystore_signal_payload(locked, lock_time)));
        }
        (locked, lock_time)
    }

    pub async fn is_keystore_unlocked(&self) -> bool {
        if !self.can_use_keystore_session() {
            return false;
        }
        matches!(crate::data::read_unlock_lease(), Ok(Some(_)))
    }

    pub async fn touch_keystore_session(&self) {
        // No-op: file-based unlock leases are not refreshed on access.
    }

    pub async fn set_keystore_unlocked(&self, password: String) {
        if !self.can_use_keystore_session() {
            tracing::warn!("Ignoring keystore unlock because keystore is unavailable in this runtime mode");
            return;
        }
        if let Err(err) = crate::data::write_unlock_lease(&password, crate::data::default_unlock_ttl()) {
            tracing::error!("Failed to write keystore unlock lease: {}", err);
            return;
        }
        {
            let mut rate_limit = self.keystore_rate_limit.lock().expect("rate limit lock");
            rate_limit.failed_attempts = 0;
            rate_limit.blocked_until_epoch = None;
            rate_limit.last_seen_unlocked = Some(true);
            rate_limit.last_lock_transition_epoch = Some(now_epoch_seconds());
        }
        self.publish_event(signal_event(self.keystore_signal_payload().await));
        tracing::info!("Keystore authentication succeeded for the local user session");
    }

    pub async fn set_keystore_locked(&self, reason: &str) {
        if !self.can_use_keystore_session() {
            return;
        }
        if let Err(err) = crate::data::clear_unlock_lease() {
            tracing::error!("Failed to clear keystore unlock lease: {}", err);
        }
        {
            let mut rate_limit = self.keystore_rate_limit.lock().expect("rate limit lock");
            rate_limit.last_seen_unlocked = Some(false);
            rate_limit.last_lock_transition_epoch = Some(now_epoch_seconds());
        }
        self.publish_event(signal_event(self.keystore_signal_payload().await));
        tracing::info!("Keystore locked for the local user session: {reason}");
    }

    pub async fn record_keystore_failed_attempt(&self) -> Option<i64> {
        if !self.can_use_keystore_session() {
            return None;
        }
        let blocked_until = {
            let mut rate_limit = self.keystore_rate_limit.lock().expect("rate limit lock");
            rate_limit.failed_attempts = rate_limit.failed_attempts.saturating_add(1);
            let block_seconds = rate_limit.current_backoff_seconds();
            if block_seconds > 0 {
                rate_limit.blocked_until_epoch = Some(now_epoch_seconds() + block_seconds as i64);
            }
            rate_limit.blocked_until_epoch
        };
        tracing::warn!("Keystore authentication failed for the local user session");
        self.publish_event(signal_event(self.keystore_signal_payload().await));
        blocked_until
    }

    pub async fn keystore_signal_payload(&self) -> String {
        if !self.can_use_keystore_session() {
            return r#"{"keystore":{"locked":true,"lock_time":0}}"#.to_string();
        }
        let (locked, lock_time, _) = self.observe_keystore_lock_state();
        Self::render_keystore_signal_payload(locked, lock_time)
    }

    pub async fn keystore_blocked_until(&self) -> Option<i64> {
        if !self.can_use_keystore_session() {
            return None;
        }
        let rate_limit = self.keystore_rate_limit.lock().expect("rate limit lock");
        let blocked_until = rate_limit.blocked_until_epoch;
        let now = now_epoch_seconds();
        if let Some(until) = blocked_until
            && until <= now
        {
            return None;
        }
        blocked_until
    }

    pub async fn keystore_password(&self) -> Option<String> {
        if !self.can_use_keystore_session() {
            return None;
        }
        match crate::data::get_password_from_unlock_file() {
            Ok(Some(password)) => Some(password),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn keystore_failed_attempts(&self) -> u32 {
        self.keystore_rate_limit
            .lock()
            .expect("rate limit lock")
            .failed_attempts
    }
}

#[derive(Serialize)]
pub(crate) struct KeystoreStatusResponse {
    locked: bool,
    expires_at_epoch: Option<i64>,
}

pub async fn status(State(state): State<Arc<ServerState>>) -> impl IntoResponse {
    if !state.can_use_keystore_session() {
        return axum::Json(KeystoreStatusResponse {
            locked: true,
            expires_at_epoch: None,
        });
    }
    match crate::data::read_unlock_lease() {
        Ok(Some(lease)) => axum::Json(KeystoreStatusResponse {
            locked: false,
            expires_at_epoch: Some(lease.expires_at_epoch),
        }),
        _ => axum::Json(KeystoreStatusResponse {
            locked: true,
            expires_at_epoch: None,
        }),
    }
}

#[derive(Deserialize, Default)]
pub(crate) struct KeystoreForm {
    #[serde(default)]
    password: String,
    #[serde(default)]
    confirm: Option<String>,
}

pub async fn get_unlock_modal(State(state): State<Arc<ServerState>>) -> Response {
    if !keystore_exists().unwrap_or(false) {
        return get_bootstrap_modal(State(state)).await;
    }
    let modal = KeystoreUnlockModal {};
    match modal.render() {
        Ok(html) => state.publish_event(append_body_event(html)),
        Err(err) => state.publish_event(html_event(format!("<div>Error: {}</div>", err))),
    }
    axum::http::StatusCode::NO_CONTENT.into_response()
}

pub async fn get_process_unlock_modal(State(state): State<Arc<ServerState>>) -> Response {
    if !keystore_exists().unwrap_or(false) {
        return get_bootstrap_modal(State(state)).await;
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
            tracing::warn!("Unable to inspect hosts.yml for plaintext secret migration: {}", err);
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

async fn missing_keystore_response(state: &Arc<ServerState>) -> Response {
    state.publish_event(signal_event(format!(
        r#"{{"message":"{}"}}"#,
        missing_keystore_unlock_message()
    )));
    get_bootstrap_modal(State(state.clone())).await;
    axum::http::StatusCode::PRECONDITION_FAILED.into_response()
}

async fn blocked_unlock_response(state: &Arc<ServerState>) -> Option<Response> {
    let blocked_until = state.keystore_blocked_until().await?;
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

pub async fn get_bootstrap_modal(State(state): State<Arc<ServerState>>) -> Response {
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

pub async fn bootstrap(State(state): State<Arc<ServerState>>, Form(form): Form<KeystoreForm>) -> Response {
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
        state.publish_event(signal_event(r#"{"keystore":{"password":"","invalid":true}}"#));
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

    state.set_keystore_unlocked(password).await;
    state.publish_event(signal_event(
        r#"{"keystore":{"password":"","invalid":false,"confirm":false}}"#,
    ));
    state.publish_event(execute_script_event(
        "if (window.location.pathname === '/settings') { window.location.reload(); }",
    ));
    state.publish_event(execute_script_event(
        "document.getElementById('keystore-bootstrap-modal')?.remove();",
    ));
    axum::http::StatusCode::NO_CONTENT.into_response()
}

pub async fn unlock(State(state): State<Arc<ServerState>>, Form(form): Form<KeystoreForm>) -> Response {
    if !keystore_exists().unwrap_or(false) {
        return missing_keystore_response(&state).await;
    }

    if let Some(response) = blocked_unlock_response(&state).await {
        return response;
    }

    let password = form.password.trim().to_string();
    if password.is_empty() {
        state.publish_event(signal_event(r#"{"message":"Keystore password is required."}"#));
        state.publish_event(signal_event(r#"{"keystore":{"password":"","invalid":true}}"#));
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    }

    match authenticate(&password) {
        Ok(_) => {
            state.set_keystore_unlocked(password).await;
            state.publish_event(signal_event(r#"{"keystore":{"password":"","invalid":false}}"#));
            state.publish_event(execute_script_event(
                "if (window.location.pathname === '/settings') { window.location.reload(); }",
            ));
            state.publish_event(execute_script_event(
                "document.getElementById('keystore-unlock-modal')?.remove();",
            ));
            state.publish_event(execute_script_event(
                "document.getElementById('keystore-process-unlock-modal')?.remove();",
            ));
            axum::http::StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => {
            state.record_keystore_failed_attempt().await;
            state.publish_event(signal_event(r#"{"message":"Invalid keystore password. Try again."}"#));
            state.publish_event(signal_event(r#"{"keystore":{"password":"","invalid":true}}"#));
            tracing::warn!("Keystore unlock failed: {}", err);
            axum::http::StatusCode::UNAUTHORIZED.into_response()
        }
    }
}

pub async fn lock(State(state): State<Arc<ServerState>>) -> impl IntoResponse {
    state.set_keystore_locked("manual").await;
    state.publish_event(execute_script_event(
        "if (window.location.pathname === '/settings') { window.location.reload(); }",
    ));
    axum::http::StatusCode::NO_CONTENT
}

pub async fn ensure_unlocked_for_active_output(state: &Arc<ServerState>) -> Result<(), String> {
    if !cfg!(feature = "keystore") || !state.runtime_mode_policy.allows_local_runtime_features() {
        // When the keystore is unavailable, the active exporter can still be valid if its
        // credentials were provided directly by the runtime environment instead of local
        // keystore-backed settings. In that mode there is nothing to unlock here.
        return Ok(());
    }

    let exporter = state.exporter.read().await.clone();
    let hosts_by_name = KnownHost::parse_hosts_yml().unwrap_or_default();
    let send_hosts: Vec<String> = hosts_by_name
        .iter()
        .filter(|(_, h)| h.has_role(HostRole::Send))
        .map(|(name, _)| name.clone())
        .collect();
    let preferred_target = Settings::load().ok().and_then(|settings| settings.active_target);
    let (_output_options, selected_output, _exporter_label) =
        template::build_footer_output_context(&hosts_by_name, &send_hosts, &exporter, preferred_target.as_deref());

    if !template::active_output_requires_keystore(&hosts_by_name, &send_hosts, &selected_output, &exporter) {
        return Ok(());
    }

    if !state.is_keystore_unlocked().await {
        return Err("Keystore is locked. Unlock it before processing secure outputs.".to_string());
    }

    state.touch_keystore_session().await;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::KeystoreRateLimit;
    use super::{
        KeystoreForm, bootstrap, ensure_unlocked_for_active_output, get_process_unlock_modal, get_unlock_modal, lock,
        unlock,
    };
    use crate::{
        data::{KnownHost, Settings, authenticate},
        exporter::Exporter,
        server::{RuntimeMode, RuntimeModePolicy, ServerEvent, ServerState, Stats, test_server_state},
    };
    use axum::{
        extract::{Form, State},
        http::StatusCode,
        response::IntoResponse,
    };
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
        crate::data::write_hosts_yml_for_tests(&hosts).expect("write hosts");
    }

    fn test_service_state() -> Arc<ServerState> {
        let (stats_updates_tx, stats_updates_rx) = watch::channel(0u64);
        let runtime_mode = RuntimeMode::Service;
        Arc::new(ServerState {
            exporter: Arc::new(RwLock::new(Exporter::default())),
            kibana_url: Arc::new(RwLock::new(String::new())),
            workflow_jobs: Arc::new(RwLock::new(HashMap::new())),
            retained_bundles: Arc::new(RwLock::new(HashMap::new())),
            runtime_mode,
            runtime_mode_policy: RuntimeModePolicy::new(runtime_mode),
            keystore_rate_limit: Arc::new(std::sync::Mutex::new(KeystoreRateLimit::default())),
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
            Form(KeystoreForm {
                password: "pw".to_string(),
                confirm: None,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(!state.keystore_status().await.0, "repeat unlock should remain unlocked");
    }

    #[tokio::test]
    async fn unlock_rejects_invalid_password_with_401_and_stays_locked() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();
        authenticate("pw").expect("create keystore");

        let state = test_server_state();
        let response = unlock(
            State(state.clone()),
            Form(KeystoreForm {
                password: "wrong".to_string(),
                confirm: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(state.keystore_status().await.0, "state should remain locked");
    }

    #[tokio::test]
    async fn unlock_requires_existing_keystore() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, keystore_path) = setup_env();
        assert!(!keystore_path.exists(), "keystore should start missing");

        let state = test_server_state();
        let response = unlock(
            State(state),
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
            KnownHost::new_legacy_apikey(
                crate::data::Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![crate::data::HostRole::Send],
                None,
                false,
                None,
                Some("plaintext-api-key".to_string()),
            ),
        );
        write_hosts(hosts);

        let state = test_server_state();
        let mut events = state.subscribe_events();
        let response = unlock(
            State(state),
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
        let response = get_unlock_modal(State(state)).await;
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
        let response = get_process_unlock_modal(State(state)).await;
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
        let response = get_unlock_modal(State(state)).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(hosts_path.is_file(), "hosts file should be created when inspected");

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
            KnownHost::new_legacy_apikey(
                crate::data::Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![crate::data::HostRole::Send],
                None,
                false,
                None,
                Some("plaintext-api-key".to_string()),
            ),
        );
        write_hosts(hosts);

        let state = test_server_state();
        let mut events = state.subscribe_events();
        let response = get_unlock_modal(State(state)).await;
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
            KnownHost::new_legacy_apikey(
                crate::data::Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![crate::data::HostRole::Send],
                None,
                false,
                None,
                Some("plaintext-api-key".to_string()),
            ),
        );
        write_hosts(hosts);

        let state = test_server_state();
        let response = bootstrap(
            State(state),
            Form(KeystoreForm {
                password: "secretpw".to_string(),
                confirm: Some("on".to_string()),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(keystore_path.is_file(), "keystore should be created");

        let unlock_path = crate::data::get_unlock_path().expect("unlock path");
        assert!(unlock_path.exists(), "bootstrap should write unlock lease file");

        let migrated_hosts = KnownHost::parse_hosts_yml().expect("reload migrated hosts");
        let migrated = migrated_hosts.get("legacy-es").expect("migrated host");
        assert!(migrated.legacy_apikey.is_none(), "plaintext apikey should be scrubbed");
        assert_eq!(migrated.secret.as_deref(), Some("legacy-es"));
    }

    #[tokio::test]
    async fn lock_is_idempotent_and_preserves_locked_state() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();
        let state = test_server_state();
        state.set_keystore_unlocked("pw".to_string()).await;

        let response = lock(State(state.clone())).await.into_response();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(state.keystore_status().await.0, "lock should relock state");

        let response = lock(State(state.clone())).await.into_response();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(state.keystore_status().await.0, "repeat lock stays locked");
    }

    #[tokio::test]
    async fn keystore_lock_time_stays_stable_without_state_transition() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();
        let state = test_server_state();

        let first_status = state.keystore_status().await;
        let first_signal = state.keystore_signal_payload().await;

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let second_status = state.keystore_status().await;
        let second_signal = state.keystore_signal_payload().await;

        assert_eq!(first_status, second_status);
        assert_eq!(first_signal, second_signal);
    }

    #[tokio::test]
    async fn unlock_writes_file_based_lease() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();
        authenticate("pw").expect("create keystore");

        let state = test_server_state();
        unlock(
            State(state.clone()),
            Form(KeystoreForm {
                password: "pw".to_string(),
                confirm: None,
            }),
        )
        .await;

        let unlock_path = crate::data::get_unlock_path().expect("unlock path");
        assert!(unlock_path.exists(), "unlock file should be written on disk");

        let lease = crate::data::read_unlock_lease()
            .expect("read lease")
            .expect("lease should exist");
        let now = chrono::Utc::now().timestamp();
        assert!(lease.expires_at_epoch > now, "lease should expire in the future");
    }

    #[tokio::test]
    async fn secure_output_requests_refresh_session_and_noauth_bypasses_unlock() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();

        let noauth_host = KnownHost::new_no_auth(
            crate::data::Product::Elasticsearch,
            Url::parse("http://localhost:9200").expect("url"),
            vec![crate::data::HostRole::Send],
            None,
            false,
        );
        let secure_host = KnownHost::new_legacy_basic(
            crate::data::Product::Elasticsearch,
            Url::parse("https://secure.example.com:9200").expect("url"),
            vec![crate::data::HostRole::Send],
            None,
            false,
            Some("secure".to_string()),
            None,
        );

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
        *state.exporter.write().await = crate::exporter::Exporter::try_from(noauth_host).expect("noauth exporter");
        assert!(
            ensure_unlocked_for_active_output(&state).await.is_ok(),
            "NoAuth output should bypass keystore preflight"
        );

        settings.active_target = Some("secure".to_string());
        settings.save().expect("save secure settings");
        *state.exporter.write().await = crate::exporter::Exporter::try_from(KnownHost::new_no_auth(
            crate::data::Product::Elasticsearch,
            Url::parse("https://secure.example.com:9200").expect("secure url"),
            vec![crate::data::HostRole::Send],
            None,
            false,
        ))
        .expect("secure exporter");
        assert!(
            ensure_unlocked_for_active_output(&state).await.is_err(),
            "secure output should require unlock"
        );

        state.set_keystore_unlocked("pw".to_string()).await;
        ensure_unlocked_for_active_output(&state)
            .await
            .expect("secure output should pass once unlocked");
    }

    #[tokio::test]
    async fn service_mode_non_secure_output_bypasses_keystore_preflight() {
        let state = test_service_state();
        *state.exporter.write().await = crate::exporter::Exporter::try_from(KnownHost::new_no_auth(
            crate::data::Product::Elasticsearch,
            Url::parse("http://localhost:9200").expect("url"),
            vec![crate::data::HostRole::Send],
            None,
            false,
        ))
        .expect("noauth exporter");

        assert!(
            ensure_unlocked_for_active_output(&state).await.is_ok(),
            "service mode should allow non-secure outputs without keystore access"
        );
    }

    #[tokio::test]
    async fn service_mode_secure_output_bypasses_unlock_and_does_not_touch_local_runtime_features() {
        let _guard = env_lock().lock().expect("env lock");
        let (tmp, hosts_path, _keystore_path) = setup_env();
        let settings_path = tmp.path().join(".esdiag").join("settings.yml");
        unsafe {
            std::env::set_var("ESDIAG_SETTINGS", &settings_path);
        }

        let state = test_service_state();
        *state.exporter.write().await = crate::exporter::Exporter::try_from(KnownHost::new_legacy_apikey(
            crate::data::Product::Elasticsearch,
            Url::parse("https://secure.example.com:9200").expect("url"),
            vec![crate::data::HostRole::Send],
            None,
            false,
            None,
            Some("secret".to_string()),
        ))
        .expect("secure exporter");

        let result = ensure_unlocked_for_active_output(&state).await;

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
        assert!(state.keystore_failed_attempts() >= 12);
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
}
