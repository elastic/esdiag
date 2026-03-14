// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

mod api;
mod api_key;
mod assets;
mod docs;
mod file_upload;
#[cfg(feature = "keystore")]
mod hosts;
mod index;
#[cfg(feature = "keystore")]
mod keystore;
mod service_link;
mod settings;
mod stats;
mod template;
mod theme;

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
    routing::{get, patch, post},
};
use bytes::Bytes;
use clap::ValueEnum;
use datastar::prelude::{ElementPatchMode, PatchElements, PatchSignals};
use eyre::Result;
use eyre::eyre;
use futures::stream;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, convert::Infallible, net::SocketAddr, sync::Arc};
use tokio::sync::{RwLock, broadcast, mpsc, watch};

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

    pub fn allows_local_artifacts(&self) -> bool {
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
            keys: Arc::new(RwLock::new(HashMap::new())),
            kibana_url: Arc::new(RwLock::new(kibana_url)),
            links: Arc::new(RwLock::new(HashMap::new())),
            signals: Arc::new(RwLock::new(Signals::default())),
            stats: Arc::new(RwLock::new(Stats::default())),
            uploads: Arc::new(RwLock::new(HashMap::new())),
            shutdown: shutdown_rx,
            event_tx,
            stats_updates_tx,
            stats_updates_rx,
            runtime_mode,
            runtime_mode_policy,
            keystore_state: Arc::new(RwLock::new(HashMap::new())),
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
                .route("/docs/{*path}", get(docs::handler))
                .route("/docs", get(docs::handler_index))
                .route("/upload/process", post(file_upload::process))
                .route("/upload/submit", post(file_upload::submit))
                .route("/events", patch(events));

            let app = app
                .route("/settings/modal", get(settings::get_modal))
                .route("/api/settings/update", post(settings::update_settings));

            #[cfg(feature = "keystore")]
            let app = if runtime_mode_policy.allows_local_artifacts() {
                app.route("/settings", get(hosts::page))
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
                    .route("/keystore/unlock-and-run", post(keystore::unlock_and_run))
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
            log::info!("Starting server bind to {:?}", addr);
            match axum_server::bind(addr)
                .handle(handle_clone)
                .serve(app.with_state(state).into_make_service())
                .await
            {
                Ok(_) => log::info!("Server shutdown"),
                Err(e) => log::error!("Server error: {}", e),
            }
        });

        // wait for the server to bind
        let bound_addr = handle
            .listening()
            .await
            .ok_or_else(|| eyre::eyre!("Server failed to bind"))?;
        log::info!(
            "Starting {}-mode server on port {}",
            runtime_mode,
            bound_addr.port()
        );
        log::debug!(
            "Runtime mode policy => requires_iap_headers={}, allows_local_artifacts={}, allows_exporter_updates={}, allows_host_management={}",
            runtime_mode_policy.requires_iap_headers(),
            runtime_mode_policy.allows_local_artifacts(),
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
            log::debug!("Server thread stopped");
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

        log::info!("Server shut down");
    }
}

pub struct ServerState {
    pub exporter: Arc<RwLock<Exporter>>,
    pub kibana_url: Arc<RwLock<String>>,
    pub signals: Arc<RwLock<Signals>>,
    pub uploads: Arc<RwLock<HashMap<u64, (String, Bytes)>>>,
    pub links: Arc<RwLock<HashMap<u64, (Identifiers, Uri)>>>,
    pub keys: Arc<RwLock<HashMap<u64, (Identifiers, KnownHost)>>>,
    pub runtime_mode: RuntimeMode,
    pub runtime_mode_policy: RuntimeModePolicy,
    pub keystore_state: Arc<RwLock<HashMap<String, KeystoreSessionState>>>,
    stats: Arc<RwLock<Stats>>,
    shutdown: watch::Receiver<bool>,
    event_tx: broadcast::Sender<ServerEvent>,
    stats_updates_tx: watch::Sender<u64>,
    stats_updates_rx: watch::Receiver<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeystoreSessionState {
    pub locked: bool,
    pub lock_time: i64,
    pub failed_attempts: u32,
    pub blocked_until_epoch: Option<i64>,
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
            log::info!("Keystore session timed out and was locked");
        }
    }
}

fn now_epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs() as i64
}

fn default_keystore_user() -> &'static str {
    "Anonymous"
}

fn normalize_keystore_user(user: &str) -> String {
    let trimmed = user.trim();
    if trimmed.is_empty() {
        default_keystore_user().to_string()
    } else {
        trimmed.to_string()
    }
}

impl ServerState {
    fn apply_keystore_timeout_locked(state: &mut KeystoreSessionState) -> bool {
        let was_locked = state.locked;
        state.apply_timeout();
        !was_locked && state.locked
    }

    fn keystore_user_key(&self, user: &str) -> String {
        if self.runtime_mode_policy.requires_iap_headers() {
            normalize_keystore_user(user)
        } else {
            default_keystore_user().to_string()
        }
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
        let user = self.keystore_user_key(user);
        let mut states = self.keystore_state.write().await;
        let state = states.entry(user.clone()).or_default();
        let timed_out = Self::apply_keystore_timeout_locked(state);
        let status = (state.locked, state.lock_time);
        drop(states);
        if timed_out {
            self.publish_event(signal_event(self.keystore_signal_payload_for(&user).await));
        }
        status
    }

    pub async fn keystore_status(&self) -> (bool, i64) {
        self.keystore_status_for(default_keystore_user()).await
    }

    pub async fn is_keystore_unlocked_for(&self, user: &str) -> bool {
        let user = self.keystore_user_key(user);
        let mut states = self.keystore_state.write().await;
        let state = states.entry(user.clone()).or_default();
        let timed_out = Self::apply_keystore_timeout_locked(state);
        let unlocked = !state.locked;
        drop(states);
        if timed_out {
            self.publish_event(signal_event(self.keystore_signal_payload_for(&user).await));
        }
        unlocked
    }

    pub async fn is_keystore_unlocked(&self) -> bool {
        self.is_keystore_unlocked_for(default_keystore_user()).await
    }

    pub async fn touch_keystore_session_for(&self, user: &str) {
        let user = self.keystore_user_key(user);
        let mut states = self.keystore_state.write().await;
        let state = states.entry(user.clone()).or_default();
        let timed_out = Self::apply_keystore_timeout_locked(state);
        if !state.locked {
            state.expires_at_epoch = Some(now_epoch_seconds() + (12 * 60 * 60));
            state.lock_time = now_epoch_seconds();
        }
        drop(states);
        if timed_out {
            self.publish_event(signal_event(self.keystore_signal_payload_for(&user).await));
        }
    }

    pub async fn touch_keystore_session(&self) {
        self.touch_keystore_session_for(default_keystore_user())
            .await
    }

    pub async fn set_keystore_unlocked_for(&self, user: &str, password: String) {
        let user = self.keystore_user_key(user);
        let mut states = self.keystore_state.write().await;
        let state = states.entry(user.clone()).or_default();
        state.locked = false;
        state.lock_time = now_epoch_seconds();
        state.failed_attempts = 0;
        state.blocked_until_epoch = None;
        state.unlocked_password = Some(password);
        state.expires_at_epoch = Some(now_epoch_seconds() + (12 * 60 * 60));
        drop(states);
        self.publish_event(signal_event(self.keystore_signal_payload_for(&user).await));
        log::info!("Keystore authentication succeeded for {user}");
    }

    pub async fn set_keystore_unlocked(&self, password: String) {
        self.set_keystore_unlocked_for(default_keystore_user(), password)
            .await
    }

    pub async fn set_keystore_locked_for(&self, user: &str, reason: &str) {
        let user = self.keystore_user_key(user);
        let mut states = self.keystore_state.write().await;
        let state = states.entry(user.clone()).or_default();
        Self::apply_keystore_timeout_locked(state);
        state.locked = true;
        state.lock_time = now_epoch_seconds();
        state.unlocked_password = None;
        state.expires_at_epoch = None;
        drop(states);
        self.publish_event(signal_event(self.keystore_signal_payload_for(&user).await));
        log::info!("Keystore locked for {user}: {reason}");
    }

    pub async fn set_keystore_locked(&self, reason: &str) {
        self.set_keystore_locked_for(default_keystore_user(), reason)
            .await
    }

    pub async fn record_keystore_failed_attempt_for(&self, user: &str) -> Option<i64> {
        let user = self.keystore_user_key(user);
        let mut states = self.keystore_state.write().await;
        let state = states.entry(user.clone()).or_default();
        Self::apply_keystore_timeout_locked(state);
        state.failed_attempts = state.failed_attempts.saturating_add(1);
        let block_seconds = state.current_backoff_seconds();
        if block_seconds > 0 {
            state.blocked_until_epoch = Some(now_epoch_seconds() + block_seconds as i64);
        }
        let blocked_until = state.blocked_until_epoch;
        drop(states);
        log::warn!("Keystore authentication failed for {user}");
        self.publish_event(signal_event(self.keystore_signal_payload_for(&user).await));
        blocked_until
    }

    pub async fn record_keystore_failed_attempt(&self) -> Option<i64> {
        self.record_keystore_failed_attempt_for(default_keystore_user())
            .await
    }

    pub async fn keystore_signal_payload_for(&self, user: &str) -> String {
        let user = self.keystore_user_key(user);
        let mut states = self.keystore_state.write().await;
        let state = states.entry(user).or_default();
        Self::apply_keystore_timeout_locked(state);
        let locked = state.locked;
        let lock_time = state.lock_time;
        drop(states);
        format!(
            r#"{{"keystore":{{"locked":{},"lock_time":{}}}}}"#,
            locked, lock_time
        )
    }

    pub async fn keystore_signal_payload(&self) -> String {
        self.keystore_signal_payload_for(default_keystore_user())
            .await
    }

    pub async fn keystore_blocked_until_for(&self, user: &str) -> Option<i64> {
        let user = self.keystore_user_key(user);
        let mut states = self.keystore_state.write().await;
        let state = states.entry(user.clone()).or_default();
        let timed_out = Self::apply_keystore_timeout_locked(state);
        let blocked_until = state.blocked_until_epoch;
        drop(states);
        if timed_out {
            self.publish_event(signal_event(self.keystore_signal_payload_for(&user).await));
        }
        blocked_until
    }

    pub async fn keystore_blocked_until(&self) -> Option<i64> {
        self.keystore_blocked_until_for(default_keystore_user())
            .await
    }

    pub async fn keystore_password_for(&self, user: &str) -> Option<String> {
        let user = self.keystore_user_key(user);
        let mut states = self.keystore_state.write().await;
        let state = states.entry(user.clone()).or_default();
        let timed_out = Self::apply_keystore_timeout_locked(state);
        let password = if state.locked {
            None
        } else {
            state.unlocked_password.clone()
        };
        drop(states);
        if timed_out {
            self.publish_event(signal_event(self.keystore_signal_payload_for(&user).await));
        }
        password
    }

    pub async fn keystore_password(&self) -> Option<String> {
        self.keystore_password_for(default_keystore_user()).await
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

    pub async fn push_key(
        &self,
        id: u64,
        identifiers: Identifiers,
        host: KnownHost,
    ) -> Option<(Identifiers, KnownHost)> {
        self.keys.write().await.insert(id, (identifiers, host))
    }

    pub async fn pop_key(&self, id: u64) -> Option<(Identifiers, KnownHost)> {
        log::debug!("Popping api key id: {id}");
        self.keys.write().await.remove(&id)
    }

    pub async fn push_link(
        &self,
        id: u64,
        identifiers: Identifiers,
        uri: Uri,
    ) -> Option<(Identifiers, Uri)> {
        log::debug!("Pushing service link id: {id}");
        self.links.write().await.insert(id, (identifiers, uri))
    }

    pub async fn pop_link(&self, id: u64) -> Option<(Identifiers, Uri)> {
        log::debug!("Popping service link id: {id}");
        self.links.write().await.remove(&id)
    }

    pub async fn push_upload(
        &self,
        id: u64,
        filename: String,
        data: Bytes,
    ) -> Option<(String, Bytes)> {
        log::debug!("Pushing file upload id: {id}");
        self.uploads.write().await.insert(id, (filename, data))
    }

    pub async fn pop_upload(&self, id: u64) -> Option<(String, Bytes)> {
        log::debug!("Popping file upload id: {id}");
        self.uploads.write().await.remove(&id)
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
        self.keystore_state
            .read()
            .await
            .get(default_keystore_user())
            .and_then(|state| state.expires_at_epoch)
    }

    #[cfg(test)]
    pub(crate) async fn keystore_failed_attempts(&self) -> u32 {
        self.keystore_state
            .read()
            .await
            .get(default_keystore_user())
            .map(|state| state.failed_attempts)
            .unwrap_or(0)
    }

    fn notify_stats_changed(&self) {
        let next = *self.stats_updates_tx.borrow() + 1;
        let _ = self.stats_updates_tx.send(next);
    }
}

async fn require_authenticated_user(
    State(state): State<Arc<ServerState>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let path_is_routable_without_iap =
        request.method() == axum::http::Method::OPTIONS || path.starts_with("/keystore/");
    if state.runtime_mode_policy.requires_iap_headers()
        && !path_is_routable_without_iap
        && let Err(err) = state.resolve_user_email(request.headers())
    {
        log::warn!("Rejected unauthenticated request: {}", err);
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
        keys: Arc::new(RwLock::new(HashMap::new())),
        kibana_url: Arc::new(RwLock::new(String::new())),
        links: Arc::new(RwLock::new(HashMap::new())),
        signals: Arc::new(RwLock::new(Signals::default())),
        stats: Arc::new(RwLock::new(Stats::default())),
        uploads: Arc::new(RwLock::new(HashMap::new())),
        runtime_mode,
        runtime_mode_policy: RuntimeModePolicy::new(runtime_mode),
        keystore_state: Arc::new(RwLock::new(HashMap::new())),
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

#[derive(Deserialize)]
pub struct Signals {
    pub auth: Auth,
    pub processing: bool,
    pub loading: bool,
    pub message: String,
    pub metadata: Identifiers,
    pub file_upload: FileUpload,
    pub service_link: ServiceLink,
    pub es_api: EsApiKey,
    #[serde(default)]
    pub settings: settings::UpdateSettingsForm,
    #[serde(default)]
    pub keystore: KeystoreSignals,
    pub stats: Stats,
    pub tab: Tab,
}

impl Default for Signals {
    fn default() -> Self {
        Signals {
            auth: Auth { header: false },
            processing: false,
            loading: false,
            message: String::new(),
            metadata: Identifiers::default(),
            file_upload: FileUpload { job_id: 0 },
            service_link: ServiceLink {
                url: Uri::default(),
                token: String::new(),
                filename: String::new(),
            },
            es_api: EsApiKey {
                key: String::new(),
                url: Uri::default(),
            },
            settings: settings::UpdateSettingsForm::default(),
            keystore: KeystoreSignals::default(),
            stats: Stats::default(),
            tab: Tab::FileUpload,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct KeystoreSignals {
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub invalid: bool,
    #[serde(default)]
    pub confirm: bool,
    #[serde(default = "default_keystore_locked")]
    pub locked: bool,
    #[serde(default)]
    pub lock_time: i64,
}

fn default_keystore_locked() -> bool {
    true
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tab {
    FileUpload,
    ServiceLink,
    ApiKey,
}

#[derive(Deserialize)]
pub struct Auth {
    pub header: bool,
}

#[derive(Deserialize)]
pub struct EsApiKey {
    pub key: String,
    pub url: Uri,
}

#[derive(Deserialize)]
pub struct FileUpload {
    pub job_id: u64,
}

#[derive(Deserialize)]
pub struct ServiceLink {
    pub url: Uri,
    pub token: String,
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
    Template(String),
    JobFeed(String),
    AppendBody(String),
    PrependSelector { selector: String, html: String },
    ExecuteScript(String),
}

pub fn signal_event(signals: impl Into<String>) -> ServerEvent {
    ServerEvent::Signals(signals.into())
}

pub fn template_event(template: impl Template) -> ServerEvent {
    let html = template.render().expect("Failed to render template");
    ServerEvent::Template(html)
}

pub fn job_feed_event(template: impl Template) -> ServerEvent {
    let html = template.render().expect("Failed to render template");
    ServerEvent::JobFeed(html)
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
        ServerEvent::Template(html) => PatchElements::new(html).write_as_axum_sse_event(),
        ServerEvent::JobFeed(html) => PatchElements::new(html)
            .selector("#job-feed")
            .mode(ElementPatchMode::After)
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
    shutdown: watch::Receiver<bool>,
) -> impl futures::Stream<Item = Result<Event, Infallible>> {
    stream::unfold(
        (rx, initial, shutdown),
        |(mut rx, mut initial, mut shutdown)| async move {
            if *shutdown.borrow() {
                return None;
            }
            if let Some(event) = initial.take() {
                return Some((server_event_to_sse(event), (rx, initial, shutdown)));
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
                            Ok(event) => return Some((server_event_to_sse(event), (rx, initial, shutdown))),
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
) -> impl axum::response::IntoResponse {
    log::debug!("Started events stream");
    let initial_stats = state.get_stats().await;
    let initial = signal_event(format!(r#"{{"stats":{}}}"#, initial_stats));
    Sse::new(broadcast_receiver_stream(
        state.event_sender().subscribe(),
        Some(initial),
        state.shutdown_receiver(),
    ))
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
        RuntimeMode, RuntimeModePolicy, Server, ServerEvent, ServerState, Signals, Stats,
        receiver_stream, test_server_state,
    };
    use crate::exporter::Exporter;
    use axum::http::HeaderMap;
    use bytes::Bytes;
    use futures::StreamExt;
    use std::{collections::HashMap, sync::Arc};
    use tokio::{
        sync::{RwLock, broadcast, mpsc, watch},
        time::{Duration, timeout},
    };

    fn test_state(mode: RuntimeMode) -> ServerState {
        let (stats_updates_tx, stats_updates_rx) = watch::channel(0u64);
        ServerState {
            exporter: Arc::new(RwLock::new(Exporter::default())),
            kibana_url: Arc::new(RwLock::new(String::new())),
            signals: Arc::new(RwLock::new(Signals::default())),
            uploads: Arc::new(RwLock::new(HashMap::<u64, (String, Bytes)>::new())),
            links: Arc::new(RwLock::new(HashMap::new())),
            keys: Arc::new(RwLock::new(HashMap::new())),
            runtime_mode: mode,
            runtime_mode_policy: RuntimeModePolicy::new(mode),
            keystore_state: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(Stats::default())),
            shutdown: watch::channel(false).1,
            event_tx: broadcast::channel(16).0,
            stats_updates_tx,
            stats_updates_rx,
        }
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

    #[cfg(feature = "desktop")]
    #[test]
    fn signals_deserialize_without_settings_field_in_desktop_mode() {
        let payload = r#"{"loading":false,"processing":false,"tab":"file-upload","message":"","stats":{"jobs":{"total":0,"success":0,"failed":0},"docs":{"total":0,"errors":0}},"es_api":{"url":"","key":""},"service_link":{"token":"","url":"","filename":""},"file_upload":{"job_id":22775},"metadata":{"user":"Anonymous","account":"","case_number":"","opportunity":""},"auth":{"header":false},"theme":{"dark":true}}"#;

        let parsed = serde_json::from_str::<Signals>(payload);
        assert!(
            parsed.is_ok(),
            "desktop signals payload without settings should deserialize"
        );
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
        let mut events = state.subscribe_events();

        {
            let mut keystore_state = state.keystore_state.write().await;
            let session = keystore_state.entry("Anonymous".to_string()).or_default();
            session.locked = false;
            session.lock_time = 1;
            session.expires_at_epoch = Some(0);
            session.unlocked_password = Some("pw".to_string());
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
