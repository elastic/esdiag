// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

mod api;
mod api_key;
mod assets;
mod bundle_download;
mod docs;
mod file_upload;
#[cfg(feature = "keystore")]
mod hosts;
mod index;
#[cfg(feature = "keystore")]
mod keystore;
mod known_host;
#[cfg(feature = "keystore")]
mod saved_jobs;
mod service_link;
mod settings;
mod stats;
mod template;
mod theme;
mod workflow;

use super::processor::Identifiers;
use crate::{
    data::{KnownHost, Uri},
    exporter::Exporter,
};
use askama::Template;
#[cfg(feature = "keystore")]
use axum::routing::put;
use axum::{
    Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{
        HeaderMap,
        header::{HeaderName, VARY},
    },
    middleware,
    middleware::Next,
    response::IntoResponse,
    response::sse::Event,
    response::{Response, Sse},
    routing::{delete, get, patch, post},
};
use bytes::Bytes;
use clap::ValueEnum;
use datastar::prelude::{ElementPatchMode, PatchElements, PatchSignals};
use eyre::Result;
use eyre::eyre;
use futures::stream;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, convert::Infallible, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::sync::{RwLock, broadcast, mpsc, watch};
use uuid::Uuid;

type UploadReceiver = Arc<RwLock<mpsc::Receiver<(Identifiers, Bytes)>>>;
const IAP_USER_EMAIL_HEADER: &str = "X-Goog-Authenticated-User-Email";

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeMode {
    Service,
    #[default]
    User,
}

impl RuntimeMode {
    pub fn from_env(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "service" => Ok(Self::Service),
            "user" => Ok(Self::User),
            other => Err(eyre!(
                "Invalid ESDIAG_MODE value '{other}', expected 'service' or 'user'"
            )),
        }
    }
}

impl std::fmt::Display for RuntimeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeMode::Service => write!(f, "service"),
            RuntimeMode::User => write!(f, "user"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RuntimeModePolicy {
    mode: RuntimeMode,
}

impl RuntimeModePolicy {
    pub fn new(mode: RuntimeMode) -> Self {
        Self { mode }
    }

    pub fn mode(&self) -> RuntimeMode {
        self.mode
    }

    pub fn requires_iap_headers(&self) -> bool {
        self.mode == RuntimeMode::Service
    }

    pub fn allows_local_runtime_features(&self) -> bool {
        self.mode == RuntimeMode::User
    }

    pub fn allows_exporter_updates(&self) -> bool {
        self.mode == RuntimeMode::User
    }

    pub fn allows_host_management(&self) -> bool {
        self.mode == RuntimeMode::User
    }
}

#[derive(Deserialize, Serialize)]
struct UploadServiceRequest {
    metadata: Identifiers,
    token: String,
    url: String,
}

impl From<UploadServiceRequest> for Identifiers {
    fn from(request: UploadServiceRequest) -> Self {
        Identifiers { ..request.metadata }
    }
}

#[derive(Deserialize, Serialize)]
struct ApiKeyRequest {
    metadata: Identifiers,
    apikey: String,
    url: String,
}

impl From<ApiKeyRequest> for Identifiers {
    fn from(request: ApiKeyRequest) -> Self {
        Identifiers { ..request.metadata }
    }
}

#[derive(Clone)]
pub struct Server {
    server_handle: Option<Arc<tokio::task::JoinHandle<()>>>,
    pub rx: Option<UploadReceiver>,
    shutdown_tx: Option<watch::Sender<bool>>,
}

impl Server {
    pub async fn start(
        bind_addr: [u8; 4],
        port: u16,
        mut exporter: Exporter,
        kibana_url: String,
        runtime_mode: RuntimeMode,
    ) -> Result<(Self, std::net::SocketAddr)> {
        let (_, rx) = mpsc::channel::<(Identifiers, Bytes)>(1);
        let rx = Arc::new(RwLock::new(rx));
        let rx_clone = rx.clone();
        let docs_rx = exporter.get_docs_rx();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (stats_updates_tx, stats_updates_rx) = watch::channel(0u64);

        let (event_tx, _event_rx) = broadcast::channel::<ServerEvent>(256);
        let runtime_mode_policy = RuntimeModePolicy::new(runtime_mode);

        // Create shared state
        let state = Arc::new(ServerState {
            exporter: Arc::new(RwLock::new(exporter)),
            kibana_url: Arc::new(RwLock::new(kibana_url)),
            stats: Arc::new(RwLock::new(Stats::default())),
            workflow_jobs: Arc::new(RwLock::new(HashMap::new())),
            retained_bundles: Arc::new(RwLock::new(HashMap::new())),
            shutdown: shutdown_rx,
            event_tx,
            stats_updates_tx,
            stats_updates_rx,
            runtime_mode,
            runtime_mode_policy,
            keystore_state: Arc::new(RwLock::new(KeystoreSessionState::default())),
        });

        stats::spawn_stats_publisher(state.clone(), state.event_sender());

        let docs_state = state.clone();
        tokio::spawn(async move {
            let mut docs_rx = docs_rx;
            while let Some(doc_count) = docs_rx.recv().await {
                docs_state.add_docs_count(doc_count).await;
            }
        });

        let handle = axum_server::Handle::new();
        let handle_clone = handle.clone();

        // Start the Axum server
        let server_handle = tokio::spawn(async move {
            const FIVE_HUNDRED_TWELVE_MEBIBYTES: usize = 512 * 1024 * 1024;
            let app = Router::new()
                .route("/", get(index::handler))
                .route("/api/service_link", post(api::service_link))
                .route("/api/api_key", post(api::api_key))
                .route("/api_key", post(api_key::form))
                .route("/api_key/{id}", post(api_key::id))
                .route("/known_host", post(known_host::form))
                .route("/datastar.js", get(assets::datastar))
                .route("/datastar.js.map", get(assets::datastar_map))
                .route("/esdiag.svg", get(assets::logo))
                .route("/favicon.ico", get(assets::logo))
                .route("/service_link", post(service_link::form))
                .route("/service_link/{id}", post(service_link::id))
                .route("/style.css", get(assets::style))
                .route("/prism.js", get(assets::prism))
                .route("/prism-bash.js", get(assets::prism_bash))
                .route("/prism-json.js", get(assets::prism_json))
                .route("/prism-json5.js", get(assets::prism_json5))
                .route("/prism-rust.js", get(assets::prism_rust))
                .route("/prism.css", get(assets::prism_css))
                .route(
                    "/documentation-outline.js",
                    get(assets::documentation_outline),
                )
                .route("/theme-borealis.css", get(assets::theme_borealis))
                .route("/theme", post(theme::set_theme))
                .route(
                    "/workflow/download/{token}",
                    get(bundle_download::download_retained_bundle),
                )
                .route("/docs/{*path}", get(docs::handler))
                .route("/docs", get(docs::handler_index))
                .route("/upload/process", post(file_upload::process))
                .route("/upload/submit", post(file_upload::submit))
                .route("/events", patch(events));

            let app = if runtime_mode_policy.allows_local_runtime_features() {
                app.route("/workflow", get(index::workflow_page))
                    .route("/jobs", get(index::jobs_page))
            } else {
                app
            };

            let app = app
                .route("/settings/modal", get(settings::get_modal))
                .route("/api/settings/update", post(settings::update_settings));

            #[cfg(feature = "keystore")]
            let app = if runtime_mode_policy.allows_local_runtime_features() {
                app.route("/jobs/saved", get(saved_jobs::list_saved_jobs))
                    .route("/jobs/saved", post(saved_jobs::save_job))
                    .route("/jobs/saved/{name}", get(saved_jobs::load_saved_job))
                    .route("/jobs/saved/{name}", delete(saved_jobs::delete_saved_job))
                    .route("/settings", get(hosts::page))
                    .route("/settings/create", post(hosts::create_host))
                    .route("/settings/update", put(hosts::update_host))
                    .route("/settings/host/{action}/{id}", post(hosts::host_action))
                    .route(
                        "/settings/cluster/{action}/{id}",
                        post(hosts::cluster_action),
                    )
                    .route("/settings/host/upsert", post(hosts::upsert_host))
                    .route("/settings/host/delete", post(hosts::delete_host))
                    .route("/settings/secret/{action}/{id}", post(hosts::secret_action))
                    .route("/settings/secret/upsert", post(hosts::upsert_secret))
                    .route("/settings/secret/delete", post(hosts::delete_secret))
                    .route(
                        "/keystore/bootstrap-modal",
                        get(keystore::get_bootstrap_modal),
                    )
                    .route("/keystore/bootstrap", post(keystore::bootstrap))
                    .route("/keystore/modal", get(keystore::get_unlock_modal))
                    .route(
                        "/keystore/modal/process",
                        get(keystore::get_process_unlock_modal),
                    )
                    .route("/keystore/unlock", post(keystore::unlock))
                    .route("/keystore/lock", post(keystore::lock))
            } else {
                app
            };

            let app = if runtime_mode_policy.requires_iap_headers() {
                app.layer(middleware::from_fn_with_state(
                    state.clone(),
                    require_authenticated_user,
                ))
            } else {
                app
            };

            let app = app
                .layer(DefaultBodyLimit::max(FIVE_HUNDRED_TWELVE_MEBIBYTES))
                .layer(middleware::map_response(add_client_hint_headers));

            let addr = SocketAddr::from((bind_addr, port));

            // Start the server
            tracing::info!("Starting server bind to {:?}", addr);
            match axum_server::bind(addr)
                .handle(handle_clone)
                .serve(app.with_state(state).into_make_service())
                .await
            {
                Ok(_) => tracing::info!("Server shutdown"),
                Err(e) => tracing::error!("Server error: {}", e),
            }
        });

        // wait for the server to bind
        let bound_addr = handle
            .listening()
            .await
            .ok_or_else(|| eyre::eyre!("Server failed to bind"))?;
        tracing::info!(
            "Starting {}-mode server on port {}",
            runtime_mode,
            bound_addr.port()
        );
        tracing::debug!(
            "Runtime mode policy => requires_iap_headers={}, allows_local_runtime_features={}, allows_exporter_updates={}, allows_host_management={}",
            runtime_mode_policy.requires_iap_headers(),
            runtime_mode_policy.allows_local_runtime_features(),
            runtime_mode_policy.allows_exporter_updates(),
            runtime_mode_policy.allows_host_management()
        );

        Ok((
            Self {
                server_handle: Some(Arc::new(server_handle)),
                rx: Some(rx_clone),
                shutdown_tx: Some(shutdown_tx),
            },
            bound_addr,
        ))
    }

    pub async fn shutdown(&mut self) {
        if let Some(shutdown_tx) = &self.shutdown_tx {
            let _ = shutdown_tx.send(true);
        }
        // Shutdown the main server
        if let Some(handle) = self.server_handle.take() {
            Arc::try_unwrap(handle).map(|handle| handle.abort()).ok();
            tracing::debug!("Server thread stopped");
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = &self.shutdown_tx {
            let _ = shutdown_tx.send(true);
        }
        // Abort the server thread if it exists
        if let Some(handle) = self.server_handle.take() {
            Arc::try_unwrap(handle).map(|handle| handle.abort()).ok();
        }

        tracing::info!("Server shut down");
    }
}

pub struct ServerState {
    pub exporter: Arc<RwLock<Exporter>>,
    pub kibana_url: Arc<RwLock<String>>,
    pub workflow_jobs: Arc<RwLock<HashMap<u64, WorkflowJob>>>,
    pub retained_bundles: Arc<RwLock<HashMap<String, RetainedBundle>>>,
    pub runtime_mode: RuntimeMode,
    pub runtime_mode_policy: RuntimeModePolicy,
    // Keystore session state is intentionally single-user only. User mode keeps one
    // local in-memory session, and service mode never enables keystore access.
    pub keystore_state: Arc<RwLock<KeystoreSessionState>>,
    stats: Arc<RwLock<Stats>>,
    shutdown: watch::Receiver<bool>,
    event_tx: broadcast::Sender<ServerEvent>,
    stats_updates_tx: watch::Sender<u64>,
    stats_updates_rx: watch::Receiver<u64>,
}

#[derive(Clone, Debug)]
pub struct RetainedBundle {
    pub owner: String,
    pub accepted: bool,
    pub error: Option<String>,
    pub filename: Option<String>,
    pub path: Option<PathBuf>,
    pub cleanup_path: Option<PathBuf>,
    pub expires_at_epoch: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeystoreSessionState {
    pub locked: bool,
    pub lock_time: i64,
    pub failed_attempts: u32,
    pub blocked_until_epoch: Option<i64>,
    #[serde(skip)]
    pub unlock_file_seed_available: bool,
    #[serde(skip)]
    pub unlocked_password: Option<String>,
    #[serde(skip)]
    pub expires_at_epoch: Option<i64>,
}

impl Default for KeystoreSessionState {
    fn default() -> Self {
        Self {
            locked: true,
            lock_time: now_epoch_seconds(),
            failed_attempts: 0,
            blocked_until_epoch: None,
            unlock_file_seed_available: true,
            unlocked_password: None,
            expires_at_epoch: None,
        }
    }
}

impl KeystoreSessionState {
    fn current_backoff_seconds(&self) -> u64 {
        if self.failed_attempts <= 3 {
            return 0;
        }
        let over = self.failed_attempts - 3;
        let minutes = (over as u64).saturating_mul(5).min(60);
        minutes * 60
    }

    fn apply_timeout(&mut self) {
        let now = now_epoch_seconds();
        if let Some(blocked_until) = self.blocked_until_epoch
            && blocked_until <= now
        {
            self.blocked_until_epoch = None;
        }
        if let Some(expires_at) = self.expires_at_epoch
            && expires_at <= now
            && !self.locked
        {
            self.locked = true;
            self.lock_time = now;
            self.unlocked_password = None;
            self.expires_at_epoch = None;
            tracing::info!("Keystore session timed out and was locked");
        }
    }
}

fn now_epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs() as i64
}

impl ServerState {
    fn retained_bundle_signal(
        owner: &str,
        token: &str,
        status: &str,
        error: Option<&str>,
    ) -> ServerEvent {
        targeted_signal_event(
            owner,
            serde_json::json!({
                "archive": {
                    "ready_token": token,
                    "status": status,
                    "error": error.unwrap_or("")
                }
            })
            .to_string(),
        )
    }

    fn apply_keystore_timeout_locked(state: &mut KeystoreSessionState) -> bool {
        let was_locked = state.locked;
        state.apply_timeout();
        !was_locked && state.locked
    }

    fn can_use_keystore_session(&self) -> bool {
        // Keystore support is only available for the local single-user web flow.
        // Service mode is explicitly excluded, even if a request is authenticated.
        cfg!(feature = "keystore")
            && self.runtime_mode_policy.allows_local_runtime_features()
            && !self.runtime_mode_policy.requires_iap_headers()
    }

    pub async fn record_job_started(&self) {
        let mut stats = self.stats.write().await;
        stats.jobs.active += 1;
        drop(stats);
        self.notify_stats_changed();
    }

    pub fn resolve_user_email(&self, headers: &HeaderMap) -> Result<(bool, String)> {
        if self.runtime_mode_policy.requires_iap_headers() {
            let raw = headers
                .get(IAP_USER_EMAIL_HEADER)
                .ok_or_else(|| eyre!("Missing required header: {}", IAP_USER_EMAIL_HEADER))?
                .to_str()
                .map_err(|_| eyre!("Invalid {} header", IAP_USER_EMAIL_HEADER))?;
            let email = raw.split(':').next_back().unwrap_or(raw).trim().to_string();
            if email.is_empty() {
                return Err(eyre!("{} header is empty", IAP_USER_EMAIL_HEADER));
            }
            return Ok((true, email));
        }

        if let Ok(user) = std::env::var("ESDIAG_USER") {
            let user = user.trim().to_string();
            if !user.is_empty() {
                return Ok((false, user));
            }
        }

        let has_header = headers.contains_key(IAP_USER_EMAIL_HEADER);
        let email = headers
            .get(IAP_USER_EMAIL_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(|raw| raw.split(':').next_back().unwrap_or(raw).trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Anonymous".to_string());

        Ok((has_header, email))
    }

    pub async fn keystore_status_for(&self, user: &str) -> (bool, i64) {
        if !self.can_use_keystore_session() {
            return (true, 0);
        }
        self.maybe_seed_keystore_session_from_unlock_for(user).await;
        let mut state = self.keystore_state.write().await;
        let timed_out = Self::apply_keystore_timeout_locked(&mut state);
        let status = (state.locked, state.lock_time);
        drop(state);
        if timed_out {
            let _ = user;
            self.publish_event(signal_event(self.keystore_signal_payload().await));
        }
        status
    }

    pub async fn keystore_status(&self) -> (bool, i64) {
        self.keystore_status_for("single-user").await
    }

    pub async fn is_keystore_unlocked_for(&self, user: &str) -> bool {
        if !self.can_use_keystore_session() {
            return false;
        }
        self.maybe_seed_keystore_session_from_unlock_for(user).await;
        let mut state = self.keystore_state.write().await;
        let timed_out = Self::apply_keystore_timeout_locked(&mut state);
        let unlocked = !state.locked;
        drop(state);
        if timed_out {
            let _ = user;
            self.publish_event(signal_event(self.keystore_signal_payload().await));
        }
        unlocked
    }

    pub async fn is_keystore_unlocked(&self) -> bool {
        self.is_keystore_unlocked_for("single-user").await
    }

    pub async fn touch_keystore_session_for(&self, user: &str) {
        if !self.can_use_keystore_session() {
            return;
        }
        let mut state = self.keystore_state.write().await;
        let timed_out = Self::apply_keystore_timeout_locked(&mut state);
        if !state.locked {
            state.expires_at_epoch = Some(now_epoch_seconds() + (12 * 60 * 60));
        }
        drop(state);
        if timed_out {
            let _ = user;
            self.publish_event(signal_event(self.keystore_signal_payload().await));
        }
    }

    pub async fn touch_keystore_session(&self) {
        self.touch_keystore_session_for("single-user").await
    }

    pub async fn set_keystore_unlocked_for(&self, user: &str, password: String) {
        if !self.can_use_keystore_session() {
            tracing::warn!(
                "Ignoring keystore unlock because keystore is unavailable in this runtime mode"
            );
            return;
        }
        let mut state = self.keystore_state.write().await;
        state.locked = false;
        state.lock_time = now_epoch_seconds();
        state.failed_attempts = 0;
        state.blocked_until_epoch = None;
        state.unlock_file_seed_available = false;
        state.unlocked_password = Some(password);
        state.expires_at_epoch = Some(now_epoch_seconds() + (12 * 60 * 60));
        drop(state);
        self.publish_event(signal_event(self.keystore_signal_payload().await));
        let _ = user;
        tracing::info!("Keystore authentication succeeded for the local user session");
    }

    pub async fn set_keystore_unlocked(&self, password: String) {
        self.set_keystore_unlocked_for("single-user", password)
            .await
    }

    pub async fn set_keystore_locked_for(&self, user: &str, reason: &str) {
        if !self.can_use_keystore_session() {
            return;
        }
        let mut state = self.keystore_state.write().await;
        Self::apply_keystore_timeout_locked(&mut state);
        state.locked = true;
        state.lock_time = now_epoch_seconds();
        state.unlock_file_seed_available = false;
        state.unlocked_password = None;
        state.expires_at_epoch = None;
        drop(state);
        self.publish_event(signal_event(self.keystore_signal_payload().await));
        let _ = user;
        tracing::info!("Keystore locked for the local user session: {reason}");
    }

    pub async fn set_keystore_locked(&self, reason: &str) {
        self.set_keystore_locked_for("single-user", reason).await
    }

    pub async fn record_keystore_failed_attempt_for(&self, user: &str) -> Option<i64> {
        if !self.can_use_keystore_session() {
            return None;
        }
        let mut state = self.keystore_state.write().await;
        Self::apply_keystore_timeout_locked(&mut state);
        state.failed_attempts = state.failed_attempts.saturating_add(1);
        let block_seconds = state.current_backoff_seconds();
        if block_seconds > 0 {
            state.blocked_until_epoch = Some(now_epoch_seconds() + block_seconds as i64);
        }
        let blocked_until = state.blocked_until_epoch;
        drop(state);
        let _ = user;
        tracing::warn!("Keystore authentication failed for the local user session");
        self.publish_event(signal_event(self.keystore_signal_payload().await));
        blocked_until
    }

    pub async fn record_keystore_failed_attempt(&self) -> Option<i64> {
        self.record_keystore_failed_attempt_for("single-user").await
    }

    pub async fn keystore_signal_payload_for(&self, user: &str) -> String {
        if !self.can_use_keystore_session() {
            return r#"{"keystore":{"locked":true,"lock_time":0}}"#.to_string();
        }
        let mut state = self.keystore_state.write().await;
        Self::apply_keystore_timeout_locked(&mut state);
        let locked = state.locked;
        let lock_time = state.lock_time;
        drop(state);
        let _ = user;
        format!(
            r#"{{"keystore":{{"locked":{},"lock_time":{}}}}}"#,
            locked, lock_time
        )
    }

    pub async fn keystore_signal_payload(&self) -> String {
        self.keystore_signal_payload_for("single-user").await
    }

    pub async fn keystore_blocked_until_for(&self, user: &str) -> Option<i64> {
        if !self.can_use_keystore_session() {
            return None;
        }
        let mut state = self.keystore_state.write().await;
        let timed_out = Self::apply_keystore_timeout_locked(&mut state);
        let blocked_until = state.blocked_until_epoch;
        drop(state);
        if timed_out {
            let _ = user;
            self.publish_event(signal_event(self.keystore_signal_payload().await));
        }
        blocked_until
    }

    pub async fn keystore_blocked_until(&self) -> Option<i64> {
        self.keystore_blocked_until_for("single-user").await
    }

    pub async fn keystore_password_for(&self, user: &str) -> Option<String> {
        if !self.can_use_keystore_session() {
            return None;
        }
        self.maybe_seed_keystore_session_from_unlock_for(user).await;
        let mut state = self.keystore_state.write().await;
        let timed_out = Self::apply_keystore_timeout_locked(&mut state);
        let password = if state.locked {
            None
        } else {
            state.unlocked_password.clone()
        };
        drop(state);
        if timed_out {
            let _ = user;
            self.publish_event(signal_event(self.keystore_signal_payload().await));
        }
        password
    }

    pub async fn keystore_password(&self) -> Option<String> {
        self.keystore_password_for("single-user").await
    }

    async fn maybe_seed_keystore_session_from_unlock_for(&self, user: &str) {
        if !self.can_use_keystore_session() {
            return;
        }
        let (should_try, timed_out) = {
            let mut state = self.keystore_state.write().await;
            let timed_out = Self::apply_keystore_timeout_locked(&mut state);
            if !state.locked || !state.unlock_file_seed_available {
                (false, timed_out)
            } else {
                state.unlock_file_seed_available = false;
                (true, timed_out)
            }
        };
        if timed_out {
            self.publish_event(signal_event(self.keystore_signal_payload().await));
        }
        if !should_try {
            return;
        }
        let password = match crate::data::get_password_from_unlock_file() {
            Ok(Some(password)) => password,
            Ok(None) => return,
            Err(err) => {
                tracing::warn!(
                    "Failed to inspect CLI unlock lease for web session seed: {}",
                    err
                );
                return;
            }
        };
        if let Err(err) = crate::data::validate_existing_keystore_password(&password) {
            tracing::warn!(
                "Ignoring invalid CLI unlock lease for web session seed: {}",
                err
            );
            return;
        }
        self.set_keystore_unlocked_for(user, password).await;
        tracing::info!("Seeded web keystore session from existing CLI unlock lease");
    }

    pub async fn record_success(&self, _docs: u32, errors: u32) {
        let mut stats = self.stats.write().await;
        stats.docs.errors += errors as usize;
        stats.jobs.total += 1;
        stats.jobs.success += 1;
        if stats.jobs.active > 0 {
            stats.jobs.active -= 1;
        }
        drop(stats);
        self.notify_stats_changed();
    }

    pub async fn record_failure(&self) {
        let mut stats = self.stats.write().await;
        stats.jobs.total += 1;
        stats.jobs.failed += 1;
        if stats.jobs.active > 0 {
            stats.jobs.active -= 1;
        }
        drop(stats);
        self.notify_stats_changed();
    }

    pub async fn add_docs_count(&self, doc_count: usize) {
        let mut stats = self.stats.write().await;
        stats.docs.total += doc_count;
        drop(stats);
        self.notify_stats_changed();
    }

    pub async fn get_stats(&self) -> Stats {
        self.stats.read().await.clone()
    }

    pub async fn get_stats_as_signals(&self) -> String {
        serde_json::to_string(&self.get_stats().await)
            .unwrap_or_default()
            .replace('\"', "'")
    }

    pub async fn push_workflow_job(&self, id: u64, job: WorkflowJob) -> Option<WorkflowJob> {
        self.workflow_jobs.write().await.insert(id, job)
    }

    pub async fn insert_retained_bundle(
        &self,
        owner: String,
        filename: String,
        path: PathBuf,
        ttl: Duration,
    ) -> String {
        self.insert_retained_bundle_internal(None, owner, filename, path, None, ttl)
            .await
    }

    pub async fn insert_retained_bundle_with_token(
        &self,
        token: Option<&str>,
        owner: String,
        filename: String,
        path: PathBuf,
        cleanup_path: Option<PathBuf>,
        ttl: Duration,
    ) -> String {
        self.insert_retained_bundle_internal(token, owner, filename, path, cleanup_path, ttl)
            .await
    }

    async fn insert_retained_bundle_internal(
        &self,
        token: Option<&str>,
        owner: String,
        filename: String,
        path: PathBuf,
        cleanup_path: Option<PathBuf>,
        ttl: Duration,
    ) -> String {
        let owner_event = owner.clone();
        let token = token
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let expires_at_epoch = now_epoch_seconds() + ttl.as_secs() as i64;
        self.retained_bundles.write().await.insert(
            token.clone(),
            RetainedBundle {
                owner,
                accepted: true,
                error: None,
                filename: Some(filename),
                path: Some(path),
                cleanup_path,
                expires_at_epoch,
            },
        );
        self.publish_event(Self::retained_bundle_signal(
            &owner_event,
            &token,
            "ready",
            None,
        ));
        token
    }

    pub async fn accept_retained_bundle(&self, token: &str, owner: &str, ttl: Duration) {
        if token.trim().is_empty() {
            return;
        }
        let expires_at_epoch = now_epoch_seconds() + ttl.as_secs() as i64;
        let mut bundles = self.retained_bundles.write().await;
        let bundle = bundles.entry(token.to_string()).or_insert(RetainedBundle {
            owner: owner.to_string(),
            accepted: false,
            error: None,
            filename: None,
            path: None,
            cleanup_path: None,
            expires_at_epoch,
        });
        bundle.owner = owner.to_string();
        bundle.accepted = true;
        bundle.error = None;
        bundle.expires_at_epoch = expires_at_epoch;
    }

    pub async fn reject_retained_bundle(
        &self,
        token: &str,
        owner: &str,
        error: impl Into<String>,
        ttl: Duration,
    ) {
        if token.trim().is_empty() {
            return;
        }
        let error = error.into();
        let expires_at_epoch = now_epoch_seconds() + ttl.as_secs() as i64;
        let mut bundles = self.retained_bundles.write().await;
        let bundle = bundles.entry(token.to_string()).or_insert(RetainedBundle {
            owner: owner.to_string(),
            accepted: false,
            error: None,
            filename: None,
            path: None,
            cleanup_path: None,
            expires_at_epoch,
        });
        bundle.owner = owner.to_string();
        bundle.accepted = false;
        bundle.error = Some(error.clone());
        bundle.expires_at_epoch = expires_at_epoch;
        drop(bundles);
        self.publish_event(Self::retained_bundle_signal(
            owner,
            token,
            "error",
            Some(&error),
        ));
    }

    pub async fn retained_bundle(&self, token: &str) -> Option<RetainedBundle> {
        self.retained_bundles.read().await.get(token).cloned()
    }

    pub async fn touch_retained_bundle(&self, token: &str, ttl: Duration) -> bool {
        let mut bundles = self.retained_bundles.write().await;
        let Some(bundle) = bundles.get_mut(token) else {
            return false;
        };
        bundle.expires_at_epoch = now_epoch_seconds() + ttl.as_secs() as i64;
        true
    }

    pub async fn discard_retained_bundle(&self, token: &str) {
        let (path, cleanup_path) = self
            .retained_bundles
            .write()
            .await
            .remove(token)
            .map(|bundle| (bundle.path, bundle.cleanup_path))
            .unwrap_or((None, None));
        if let Some(path) = path
            && let Err(err) = tokio::fs::remove_file(&path).await
        {
            tracing::debug!(
                "Failed to remove retained bundle {}: {}",
                path.display(),
                err
            );
        }
        if let Some(path) = cleanup_path
            && let Err(err) = tokio::fs::remove_dir_all(&path).await
            && err.kind() != std::io::ErrorKind::NotFound
        {
            tracing::debug!(
                "Failed to remove retained bundle directory {}: {}",
                path.display(),
                err
            );
        }
    }

    pub fn schedule_retained_bundle_cleanup(self: &Arc<Self>, token: String, delay: Duration) {
        let state = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let should_cleanup = state
                .retained_bundle(&token)
                .await
                .map(|bundle| bundle.expires_at_epoch <= now_epoch_seconds())
                .unwrap_or(false);
            if should_cleanup {
                state.discard_retained_bundle(&token).await;
            }
        });
    }

    pub async fn pop_workflow_job(&self, id: u64) -> Option<WorkflowJob> {
        tracing::debug!("Popping workflow job id: {id}");
        self.workflow_jobs.write().await.remove(&id)
    }

    pub async fn discard_workflow_job(&self, id: u64) {
        if let Some(job) = self.workflow_jobs.write().await.remove(&id) {
            job.cleanup().await;
        }
    }

    pub async fn push_key(
        &self,
        id: u64,
        identifiers: Identifiers,
        host: KnownHost,
        diagnostic_type: String,
    ) -> Option<WorkflowJob> {
        self.push_workflow_job(
            id,
            WorkflowJob {
                identifiers,
                input: WorkflowInput::FromRemoteHost {
                    source: host.get_url().to_string(),
                    host,
                    diagnostic_type,
                },
            },
        )
        .await
    }

    pub async fn push_link(
        &self,
        id: u64,
        identifiers: Identifiers,
        filename: String,
        uri: Uri,
    ) -> Option<WorkflowJob> {
        tracing::debug!("Pushing service link id: {id}");
        self.push_workflow_job(
            id,
            WorkflowJob {
                identifiers,
                input: WorkflowInput::FromServiceLink {
                    source: filename,
                    uri,
                },
            },
        )
        .await
    }

    pub async fn push_upload(
        &self,
        id: u64,
        filename: String,
        path: PathBuf,
    ) -> Option<WorkflowJob> {
        tracing::debug!("Pushing file upload id: {id}");
        self.push_workflow_job(
            id,
            WorkflowJob {
                identifiers: Identifiers::default(),
                input: WorkflowInput::LocalArchive {
                    source: filename.clone(),
                    filename,
                    path: path.clone(),
                    cleanup_path: Some(path),
                },
            },
        )
        .await
    }

    pub fn shutdown_receiver(&self) -> watch::Receiver<bool> {
        self.shutdown.clone()
    }

    pub fn event_sender(&self) -> broadcast::Sender<ServerEvent> {
        self.event_tx.clone()
    }

    pub fn publish_event(&self, event: ServerEvent) {
        let _ = self.event_tx.send(event);
    }

    pub fn stats_updates_receiver(&self) -> watch::Receiver<u64> {
        self.stats_updates_rx.clone()
    }

    #[cfg(test)]
    pub(crate) fn subscribe_events(&self) -> broadcast::Receiver<ServerEvent> {
        self.event_tx.subscribe()
    }

    #[cfg(test)]
    pub(crate) async fn keystore_expires_at_epoch(&self) -> Option<i64> {
        self.keystore_state.read().await.expires_at_epoch
    }

    #[cfg(test)]
    pub(crate) async fn keystore_failed_attempts(&self) -> u32 {
        self.keystore_state.read().await.failed_attempts
    }

    fn notify_stats_changed(&self) {
        let next = *self.stats_updates_tx.borrow() + 1;
        let _ = self.stats_updates_tx.send(next);
    }
}

pub async fn ensure_active_output_ready(
    state: &Arc<ServerState>,
    user: &str,
) -> Result<(), String> {
    #[cfg(feature = "keystore")]
    {
        keystore::ensure_unlocked_for_active_output(state, user).await
    }
    #[cfg(not(feature = "keystore"))]
    {
        let _ = (state, user);
        Ok(())
    }
}

async fn require_authenticated_user(
    State(state): State<Arc<ServerState>>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let path_is_routable_without_iap = method == axum::http::Method::OPTIONS
        || path.starts_with("/.well-known/")
        || matches!(
            path.as_str(),
            "/datastar.js"
                | "/datastar.js.map"
                | "/documentation-outline.js"
                | "/esdiag.svg"
                | "/favicon.ico"
                | "/prism.js"
                | "/prism-bash.js"
                | "/prism-json.js"
                | "/prism-json5.js"
                | "/prism-rust.js"
                | "/prism.css"
                | "/style.css"
                | "/theme-borealis.css"
        );
    if state.runtime_mode_policy.requires_iap_headers()
        && !path_is_routable_without_iap
        && let Err(err) = state.resolve_user_email(request.headers())
    {
        tracing::warn!(
            "Rejected unauthenticated request for {} {}: {}",
            method,
            path,
            err
        );
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }

    next.run(request).await
}

#[cfg(test)]
pub(crate) fn test_server_state() -> Arc<ServerState> {
    let (stats_updates_tx, stats_updates_rx) = watch::channel(0u64);
    let runtime_mode = RuntimeMode::User;
    Arc::new(ServerState {
        exporter: Arc::new(RwLock::new(Exporter::default())),
        kibana_url: Arc::new(RwLock::new(String::new())),
        stats: Arc::new(RwLock::new(Stats::default())),
        workflow_jobs: Arc::new(RwLock::new(HashMap::new())),
        retained_bundles: Arc::new(RwLock::new(HashMap::new())),
        runtime_mode,
        runtime_mode_policy: RuntimeModePolicy::new(runtime_mode),
        keystore_state: Arc::new(RwLock::new(KeystoreSessionState::default())),
        shutdown: watch::channel(false).1,
        event_tx: broadcast::channel(16).0,
        stats_updates_tx,
        stats_updates_rx,
    })
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Stats {
    pub docs: DocStats,
    pub jobs: JobStats,
}

impl Default for Stats {
    fn default() -> Self {
        Stats {
            docs: DocStats {
                total: 0,
                errors: 0,
            },
            jobs: JobStats {
                total: 0,
                success: 0,
                failed: 0,
                active: 0,
            },
        }
    }
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let json = match serde_json::to_string(self) {
            Ok(json) => json,
            Err(_) => return Err(std::fmt::Error),
        };
        write!(f, "{}", json)
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct DocStats {
    pub total: usize,
    pub errors: usize,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct JobStats {
    pub total: u64,
    pub success: u64,
    pub failed: u64,
    #[serde(default)]
    pub active: u64,
}

#[derive(Clone, Default, Deserialize)]
pub struct WorkflowRunSignals {
    #[serde(default)]
    pub metadata: Identifiers,
    #[serde(default)]
    pub archive: ArchiveSignals,
    #[serde(default)]
    pub workflow: Workflow,
}

#[derive(Clone, Default, Deserialize)]
pub struct KnownHostFormSignals {
    #[serde(default)]
    pub metadata: Identifiers,
    #[serde(default)]
    pub archive: ArchiveSignals,
    #[serde(default)]
    pub workflow: Workflow,
}

impl From<KnownHostFormSignals> for WorkflowRunSignals {
    fn from(signals: KnownHostFormSignals) -> Self {
        Self {
            metadata: signals.metadata,
            archive: signals.archive,
            workflow: signals.workflow,
        }
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct ApiKeyFormSignals {
    #[serde(default)]
    pub metadata: Identifiers,
    #[serde(default)]
    pub archive: ArchiveSignals,
    #[serde(default)]
    pub workflow: Workflow,
    #[serde(default)]
    pub es_api: EsApiKey,
}

impl From<ApiKeyFormSignals> for WorkflowRunSignals {
    fn from(signals: ApiKeyFormSignals) -> Self {
        Self {
            metadata: signals.metadata,
            archive: signals.archive,
            workflow: signals.workflow,
        }
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct ServiceLinkFormSignals {
    #[serde(default)]
    pub metadata: Identifiers,
    #[serde(default)]
    pub archive: ArchiveSignals,
    #[serde(default)]
    pub workflow: Workflow,
    #[serde(default)]
    pub service_link: ServiceLink,
}

impl From<ServiceLinkFormSignals> for WorkflowRunSignals {
    fn from(signals: ServiceLinkFormSignals) -> Self {
        Self {
            metadata: signals.metadata,
            archive: signals.archive,
            workflow: signals.workflow,
        }
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct UploadProcessSignals {
    #[serde(default)]
    pub metadata: Identifiers,
    #[serde(default)]
    pub archive: ArchiveSignals,
    #[serde(default)]
    pub workflow: Workflow,
    #[serde(default)]
    pub file_upload: FileUpload,
}

impl From<UploadProcessSignals> for WorkflowRunSignals {
    fn from(signals: UploadProcessSignals) -> Self {
        Self {
            metadata: signals.metadata,
            archive: signals.archive,
            workflow: signals.workflow,
        }
    }
}

#[derive(Default, Deserialize)]
pub struct SettingsUpdateSignals {
    #[serde(default)]
    pub settings: settings::UpdateSettingsForm,
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct ArchiveSignals {
    #[serde(default)]
    pub download_token: String,
    #[serde(default)]
    pub pending_token: String,
    #[serde(default)]
    pub ready_token: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub error: String,
}

// Workflow types are defined in data::workflow and re-exported here for backwards compat
pub use crate::data::workflow::{
    CollectMode, CollectSource, CollectStage, ProcessMode, ProcessStage, SendMode, SendStage,
    Workflow,
};

#[derive(Clone)]
pub struct WorkflowJob {
    pub identifiers: Identifiers,
    pub input: WorkflowInput,
}

impl WorkflowJob {
    pub fn source(&self) -> &str {
        self.input.source()
    }

    pub async fn cleanup(&self) {
        self.input.cleanup().await;
    }
}

#[derive(Clone)]
pub enum WorkflowInput {
    LocalArchive {
        source: String,
        filename: String,
        path: PathBuf,
        cleanup_path: Option<PathBuf>,
    },
    FromServiceLink {
        source: String,
        uri: Uri,
    },
    FromRemoteHost {
        source: String,
        host: KnownHost,
        diagnostic_type: String,
    },
}

impl WorkflowInput {
    pub fn source(&self) -> &str {
        match self {
            Self::LocalArchive { source, .. } => source,
            Self::FromServiceLink { source, .. } => source,
            Self::FromRemoteHost { source, .. } => source,
        }
    }

    pub async fn cleanup(&self) {
        if let Self::LocalArchive {
            cleanup_path: Some(path),
            ..
        } = self
        {
            let metadata = tokio::fs::metadata(path).await;
            let result = match metadata {
                Ok(metadata) if metadata.is_dir() => tokio::fs::remove_dir_all(path).await,
                Ok(_) => tokio::fs::remove_file(path).await,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(err) => Err(err),
            };
            if let Err(err) = result {
                tracing::debug!("Failed to clean workflow input {}: {}", path.display(), err);
            }
        }
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct EsApiKey {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub url: Uri,
}

#[derive(Clone, Default, Deserialize)]
pub struct FileUpload {
    #[serde(default)]
    pub job_id: u64,
}

#[derive(Clone, Default, Deserialize)]
pub struct ServiceLink {
    #[serde(default)]
    pub url: Uri,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub filename: String,
}

pub fn patch_signals(signals: &str) -> Result<Event, Infallible> {
    let sse_event = PatchSignals::new(signals).write_as_axum_sse_event();
    Ok(sse_event)
}

pub fn patch_template(template: impl Template) -> Result<Event, Infallible> {
    let element = template.render().expect("Failed to render template");
    let sse_event = PatchElements::new(element).write_as_axum_sse_event();
    Ok(sse_event)
}

pub fn patch_job_feed(template: impl Template) -> Result<Event, Infallible> {
    let element = template.render().expect("Failed to render template");
    let sse_event = PatchElements::new(element)
        .selector("#job-feed")
        .mode(ElementPatchMode::After)
        .write_as_axum_sse_event();
    Ok(sse_event)
}

#[derive(Debug, Clone)]
pub enum ServerEvent {
    Signals(String),
    TargetedSignals { user: String, payload: String },
    Template(String),
    JobFeed(String),
    ReplaceSelector { selector: String, html: String },
    AppendBody(String),
    PrependSelector { selector: String, html: String },
    ExecuteScript(String),
}

pub fn signal_event(signals: impl Into<String>) -> ServerEvent {
    ServerEvent::Signals(signals.into())
}

pub fn targeted_signal_event(user: impl Into<String>, signals: impl Into<String>) -> ServerEvent {
    ServerEvent::TargetedSignals {
        user: user.into(),
        payload: signals.into(),
    }
}

pub fn template_event(template: impl Template) -> ServerEvent {
    let html = template.render().expect("Failed to render template");
    ServerEvent::Template(html)
}

pub fn job_feed_event(template: impl Template) -> ServerEvent {
    let html = template.render().expect("Failed to render template");
    ServerEvent::JobFeed(html)
}

pub fn replace_job_event(job_id: u64, template: impl Template) -> ServerEvent {
    let html = template.render().expect("Failed to render template");
    ServerEvent::ReplaceSelector {
        selector: format!("#job-{job_id}"),
        html,
    }
}

pub fn html_event(html: impl Into<String>) -> ServerEvent {
    ServerEvent::Template(html.into())
}

pub fn append_body_event(html: impl Into<String>) -> ServerEvent {
    ServerEvent::AppendBody(html.into())
}

pub fn prepend_selector_event(selector: impl Into<String>, html: impl Into<String>) -> ServerEvent {
    ServerEvent::PrependSelector {
        selector: selector.into(),
        html: html.into(),
    }
}

pub fn execute_script_event(script: impl Into<String>) -> ServerEvent {
    ServerEvent::ExecuteScript(script.into())
}

pub fn server_event_to_sse(event: ServerEvent) -> Result<Event, Infallible> {
    let sse_event = match event {
        ServerEvent::Signals(payload) => PatchSignals::new(payload).write_as_axum_sse_event(),
        ServerEvent::TargetedSignals { payload, .. } => {
            PatchSignals::new(payload).write_as_axum_sse_event()
        }
        ServerEvent::Template(html) => PatchElements::new(html).write_as_axum_sse_event(),
        ServerEvent::JobFeed(html) => PatchElements::new(html)
            .selector("#job-feed")
            .mode(ElementPatchMode::After)
            .write_as_axum_sse_event(),
        ServerEvent::ReplaceSelector { selector, html } => PatchElements::new(html)
            .selector(&selector)
            .mode(ElementPatchMode::Outer)
            .write_as_axum_sse_event(),
        ServerEvent::AppendBody(html) => PatchElements::new(html)
            .selector("body")
            .mode(ElementPatchMode::Append)
            .write_as_axum_sse_event(),
        ServerEvent::PrependSelector { selector, html } => PatchElements::new(html)
            .selector(&selector)
            .mode(ElementPatchMode::Prepend)
            .write_as_axum_sse_event(),
        ServerEvent::ExecuteScript(script) => {
            datastar::prelude::ExecuteScript::new(&script).write_as_axum_sse_event()
        }
    };

    Ok(sse_event)
}

pub fn receiver_stream(
    rx: mpsc::Receiver<ServerEvent>,
) -> impl futures::Stream<Item = Result<Event, Infallible>> {
    stream::unfold(rx, |mut rx| async move {
        rx.recv()
            .await
            .map(|event| (server_event_to_sse(event), rx))
    })
}

fn broadcast_receiver_stream(
    rx: broadcast::Receiver<ServerEvent>,
    initial: Option<ServerEvent>,
    user: String,
    shutdown: watch::Receiver<bool>,
) -> impl futures::Stream<Item = Result<Event, Infallible>> {
    stream::unfold(
        (rx, initial, user, shutdown),
        |(mut rx, mut initial, user, mut shutdown)| async move {
            if *shutdown.borrow() {
                return None;
            }
            if let Some(event) = initial.take() {
                return Some((server_event_to_sse(event), (rx, initial, user, shutdown)));
            }
            loop {
                tokio::select! {
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            return None;
                        }
                    }
                    recv = rx.recv() => {
                        match recv {
                            Ok(event) => {
                                if !event_visible_to_user(&event, &user) {
                                    continue;
                                }
                                return Some((server_event_to_sse(event), (rx, initial, user, shutdown)));
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(broadcast::error::RecvError::Closed) => return None,
                        }
                    }
                }
            }
        },
    )
}

async fn events(
    axum::extract::State(state): axum::extract::State<Arc<ServerState>>,
    headers: HeaderMap,
) -> impl axum::response::IntoResponse {
    tracing::debug!("Started events stream");
    let (_, request_user) = state
        .resolve_user_email(&headers)
        .unwrap_or_else(|_| (false, "Anonymous".to_string()));
    let initial_stats = state.get_stats().await;
    let initial = signal_event(format!(r#"{{"stats":{}}}"#, initial_stats));
    Sse::new(broadcast_receiver_stream(
        state.event_sender().subscribe(),
        Some(initial),
        request_user,
        state.shutdown_receiver(),
    ))
}

fn event_visible_to_user(event: &ServerEvent, user: &str) -> bool {
    match event {
        ServerEvent::TargetedSignals {
            user: target_user, ..
        } => target_user == user,
        _ => true,
    }
}

fn parse_cookie(headers: &HeaderMap, key: &str) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|raw| {
            raw.split(';').find_map(|part| {
                let trimmed = part.trim();
                let (k, v) = trimmed.split_once('=')?;
                if k == key { Some(v.to_string()) } else { None }
            })
        })
}

pub(super) fn get_theme_dark(headers: &HeaderMap) -> bool {
    if let Some(cookie_dark) = parse_cookie(headers, "theme_dark") {
        return matches!(cookie_dark.as_str(), "1" | "true");
    }

    headers
        .get("sec-ch-prefers-color-scheme")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase().contains("dark"))
        .unwrap_or(false)
}

async fn add_client_hint_headers(mut response: Response) -> Response {
    const SEC_CH_PREFERS_COLOR_SCHEME: &str = "Sec-CH-Prefers-Color-Scheme";
    const ACCEPT_CH: HeaderName = HeaderName::from_static("accept-ch");
    const CRITICAL_CH: HeaderName = HeaderName::from_static("critical-ch");

    let headers = response.headers_mut();
    headers.insert(
        ACCEPT_CH,
        SEC_CH_PREFERS_COLOR_SCHEME
            .parse()
            .expect("valid Accept-CH value"),
    );
    headers.insert(
        CRITICAL_CH,
        SEC_CH_PREFERS_COLOR_SCHEME
            .parse()
            .expect("valid Critical-CH value"),
    );
    headers.append(
        VARY,
        SEC_CH_PREFERS_COLOR_SCHEME
            .parse()
            .expect("valid Vary value"),
    );
    headers.append(VARY, "Cookie".parse().expect("valid Vary value"));

    response
}

#[cfg(test)]
mod tests {
    use super::{
        ApiKeyFormSignals, KeystoreSessionState, RuntimeMode, RuntimeModePolicy, Server,
        ServerEvent, ServerState, Stats, WorkflowRunSignals, event_visible_to_user,
        receiver_stream, replace_job_event, signal_event, targeted_signal_event, test_server_state,
    };
    use crate::data::{create_keystore, write_unlock_lease};
    use crate::exporter::Exporter;
    use axum::http::HeaderMap;
    use futures::StreamExt;
    use std::{collections::HashMap, sync::Arc};
    use tempfile::TempDir;
    use tokio::{
        sync::{RwLock, broadcast, mpsc, watch},
        time::{Duration, timeout},
    };

    fn test_state(mode: RuntimeMode) -> ServerState {
        let (stats_updates_tx, stats_updates_rx) = watch::channel(0u64);
        ServerState {
            exporter: Arc::new(RwLock::new(Exporter::default())),
            kibana_url: Arc::new(RwLock::new(String::new())),
            workflow_jobs: Arc::new(RwLock::new(HashMap::new())),
            retained_bundles: Arc::new(RwLock::new(HashMap::new())),
            runtime_mode: mode,
            runtime_mode_policy: RuntimeModePolicy::new(mode),
            keystore_state: Arc::new(RwLock::new(KeystoreSessionState::default())),
            stats: Arc::new(RwLock::new(Stats::default())),
            shutdown: watch::channel(false).1,
            event_tx: broadcast::channel(16).0,
            stats_updates_tx,
            stats_updates_rx,
        }
    }

    fn setup_keystore_env() -> TempDir {
        let tmp = TempDir::new().expect("temp dir");
        let config_dir = tmp.path().join(".esdiag");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        let keystore_path = config_dir.join("secrets.yml");
        unsafe {
            std::env::set_var("HOME", tmp.path());
            std::env::set_var("USERPROFILE", tmp.path());
            std::env::set_var("ESDIAG_KEYSTORE", &keystore_path);
            std::env::remove_var("ESDIAG_KEYSTORE_PASSWORD");
        }
        tmp
    }

    #[tokio::test]
    async fn start_with_ephemeral_port_binds_and_reports_socket() {
        let (mut server, bound_addr) = Server::start(
            [127, 0, 0, 1],
            0,
            Exporter::default(),
            String::new(),
            RuntimeMode::User,
        )
        .await
        .expect("server should bind on ephemeral port");

        assert!(bound_addr.ip().is_loopback());
        assert!(bound_addr.port() > 0);

        server.shutdown().await;
    }

    #[test]
    fn workflow_run_signals_deserialize_without_archive_field() {
        let payload = r#"{"metadata":{"user":"Anonymous","account":"","case_number":"","opportunity":""},"workflow":{"collect":{"mode":"collect","source":"known-host","known_host":"esdiag-prod","diagnostic_type":"standard","save":true,"save_dir":"/Users/reno/Downloads"},"process":{"mode":"forward","enabled":false,"product":"elasticsearch","diagnostic_type":"standard","selected":""},"send":{"mode":"remote","remote_target":"b8b9a090-21fa-419f-a731-ae8676fdd835","local_target":"directory","local_directory":"Directory /tmp/output"}}}"#;

        let parsed = serde_json::from_str::<WorkflowRunSignals>(payload)
            .expect("workflow run payload without archive should deserialize");
        assert_eq!(parsed.workflow.collect.known_host, "esdiag-prod");
        assert!(parsed.archive.download_token.is_empty());
    }

    #[test]
    fn api_key_form_signals_deserialize_without_archive_field() {
        let payload = r#"{"metadata":{"user":"Anonymous","account":"","case_number":"","opportunity":""},"workflow":{"collect":{"mode":"collect","source":"api-key","known_host":"","diagnostic_type":"standard","save":false,"save_dir":""},"process":{"mode":"process","enabled":true,"product":"elasticsearch","diagnostic_type":"standard","selected":""},"send":{"mode":"remote","remote_target":"","local_target":"","local_directory":""}},"es_api":{"url":"","key":"secret"}}"#;

        let parsed = serde_json::from_str::<ApiKeyFormSignals>(payload)
            .expect("api key payload without archive should deserialize");
        assert_eq!(parsed.es_api.key, "secret");
        assert!(parsed.archive.download_token.is_empty());
    }

    #[tokio::test]
    async fn receiver_stream_preserves_event_order() {
        let _state = test_server_state();
        let (tx, rx) = mpsc::channel(4);
        tx.send(ServerEvent::Signals(r#"{"a":1}"#.to_string()))
            .await
            .expect("send first event");
        tx.send(ServerEvent::Signals(r#"{"b":2}"#.to_string()))
            .await
            .expect("send second event");
        drop(tx);

        let events: Vec<_> = receiver_stream(rx).collect().await;
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn targeted_events_only_reach_matching_user() {
        assert!(event_visible_to_user(
            &targeted_signal_event("alice@example.com", r#"{"archive":{"status":"ready"}}"#),
            "alice@example.com"
        ));
        assert!(!event_visible_to_user(
            &targeted_signal_event("alice@example.com", r#"{"archive":{"status":"ready"}}"#),
            "bob@example.com"
        ));
        assert!(event_visible_to_user(
            &signal_event(r#"{"stats":{"jobs":{"total":1}}}"#),
            "bob@example.com"
        ));
    }

    #[test]
    fn replace_job_event_targets_matching_job_selector() {
        let event = replace_job_event(
            42,
            crate::server::template::JobFailed {
                job_id: 42,
                error: "boom",
                source: "upload.zip",
            },
        );

        match event {
            ServerEvent::ReplaceSelector { selector, html } => {
                assert_eq!(selector, "#job-42");
                assert!(html.contains(r#"id="job-42""#));
            }
            other => panic!("expected replace selector event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn events_stream_terminates_on_server_shutdown() {
        let (mut server, bound_addr) = Server::start(
            [127, 0, 0, 1],
            0,
            Exporter::default(),
            String::new(),
            RuntimeMode::User,
        )
        .await
        .expect("server should bind");
        let url = format!("http://{}/events", bound_addr);

        let client = reqwest::Client::new();
        let mut response = client
            .patch(url)
            .send()
            .await
            .expect("events request should succeed");
        assert!(response.status().is_success());

        let first = timeout(Duration::from_secs(2), response.chunk())
            .await
            .expect("stream should produce initial event");
        assert!(matches!(first, Ok(Some(_))));

        server.shutdown().await;

        let next_after_shutdown = timeout(Duration::from_secs(2), response.chunk())
            .await
            .expect("stream should terminate shortly after shutdown");
        assert!(
            matches!(next_after_shutdown, Ok(None) | Err(_)),
            "expected stream to end or error after shutdown"
        );
    }

    #[tokio::test]
    async fn keystore_timeout_status_read_publishes_locked_signal() {
        let state = test_server_state();
        let mut events = state.event_sender().subscribe();

        {
            let mut keystore_state = state.keystore_state.write().await;
            keystore_state.locked = false;
            keystore_state.lock_time = 1;
            keystore_state.expires_at_epoch = Some(0);
            keystore_state.unlocked_password = Some("pw".to_string());
        }

        let (locked, lock_time) = state.keystore_status().await;

        assert!(locked);
        assert!(lock_time > 0);

        let event = events.try_recv().expect("timeout should publish signal");
        let ServerEvent::Signals(payload) = event else {
            panic!("expected signal event");
        };
        assert!(payload.contains(r#""keystore":{"locked":true"#));
    }

    #[tokio::test]
    async fn service_mode_keystore_session_remains_disabled() {
        let state = test_state(RuntimeMode::Service);

        state
            .set_keystore_unlocked_for("alice@example.com", "pw".to_string())
            .await;

        assert_eq!(
            state.keystore_status_for("alice@example.com").await,
            (true, 0)
        );
        assert_eq!(
            state.keystore_status_for("bob@example.com").await,
            (true, 0)
        );
        assert_eq!(state.keystore_password_for("alice@example.com").await, None);
        assert_eq!(state.keystore_password_for("bob@example.com").await, None);
    }

    #[tokio::test]
    async fn user_mode_keystore_session_is_shared_across_request_users() {
        let state = test_state(RuntimeMode::User);

        state
            .set_keystore_unlocked_for("alice@example.com", "pw".to_string())
            .await;

        assert!(state.is_keystore_unlocked_for("alice@example.com").await);
        assert!(state.is_keystore_unlocked_for("bob@example.com").await);
        assert_eq!(
            state.keystore_password_for("bob@example.com").await,
            Some("pw".to_string())
        );

        state
            .set_keystore_locked_for("carol@example.com", "test")
            .await;

        assert!(!state.is_keystore_unlocked_for("alice@example.com").await);
        assert!(!state.is_keystore_unlocked_for("bob@example.com").await);
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn user_mode_can_seed_keystore_session_from_cli_unlock_file_once() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        let _tmp = setup_keystore_env();
        create_keystore("pw").expect("create keystore");
        write_unlock_lease("pw", Duration::from_secs(300)).expect("write unlock lease");

        let state = test_state(RuntimeMode::User);

        assert!(state.is_keystore_unlocked_for("alice@example.com").await);
        assert_eq!(
            state.keystore_password_for("bob@example.com").await,
            Some("pw".to_string())
        );
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn explicit_web_lock_prevents_reseeding_from_unlock_file() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        let _tmp = setup_keystore_env();
        create_keystore("pw").expect("create keystore");
        write_unlock_lease("pw", Duration::from_secs(300)).expect("write unlock lease");

        let state = test_state(RuntimeMode::User);
        assert!(state.is_keystore_unlocked().await);

        state.set_keystore_locked("manual").await;

        assert!(!state.is_keystore_unlocked().await);
        assert_eq!(state.keystore_password().await, None);
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn service_mode_does_not_seed_from_cli_unlock_file() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        let _tmp = setup_keystore_env();
        create_keystore("pw").expect("create keystore");
        write_unlock_lease("pw", Duration::from_secs(300)).expect("write unlock lease");

        let state = test_state(RuntimeMode::Service);

        assert!(!state.is_keystore_unlocked().await);
        assert_eq!(state.keystore_password().await, None);
    }

    #[tokio::test]
    async fn touch_keystore_session_preserves_lock_transition_time() {
        let state = test_state(RuntimeMode::User);

        state.set_keystore_unlocked("pw".to_string()).await;
        let first_lock_time = state.keystore_status().await.1;
        tokio::time::sleep(Duration::from_secs(1)).await;

        state.touch_keystore_session().await;

        assert_eq!(state.keystore_status().await.1, first_lock_time);
        assert!(
            state.keystore_expires_at_epoch().await.is_some(),
            "touch should still extend the session lease"
        );
    }

    #[test]
    fn service_mode_requires_iap_header() {
        let state = test_state(RuntimeMode::Service);
        let headers = HeaderMap::new();
        assert!(
            state.resolve_user_email(&headers).is_err(),
            "service mode should reject missing IAP header"
        );
    }

    #[test]
    fn service_mode_extracts_iap_email() {
        let state = test_state(RuntimeMode::Service);
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Goog-Authenticated-User-Email",
            "accounts.google.com:ops@example.com"
                .parse()
                .expect("valid header"),
        );
        let (_, user) = state
            .resolve_user_email(&headers)
            .expect("service mode should parse user");
        assert_eq!(user, "ops@example.com");
    }

    #[test]
    fn user_mode_allows_missing_header() {
        let state = test_state(RuntimeMode::User);
        let headers = HeaderMap::new();
        let (_, user) = state
            .resolve_user_email(&headers)
            .expect("user mode should not require header");
        assert_eq!(user, "Anonymous");
    }
}
