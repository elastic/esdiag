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
mod job_runner;
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

use super::processor::Identifiers;
use crate::{
    data::{KnownHost, Uri},
    exporter::Exporter,
};
use askama::Template;
#[cfg(feature = "keystore")]
use axum::routing::{delete, put};
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
use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};
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

#[derive(Clone, Debug)]
pub struct ServerPolicy {
    mode: RuntimeMode,
    web_features: WebFeatureSet,
}

impl ServerPolicy {
    pub fn defaults(mode: RuntimeMode) -> Self {
        Self {
            mode,
            web_features: WebFeatureSet::defaults_for(mode),
        }
    }

    pub fn new(mode: RuntimeMode) -> Result<Self> {
        Self::with_web_features(mode, None)
    }

    pub fn with_web_features(mode: RuntimeMode, web_features: Option<&str>) -> Result<Self> {
        let web_features = match web_features {
            Some(value) => WebFeatureSet::parse(value)?,
            None => match std::env::var("ESDIAG_WEB_FEATURES") {
                Ok(value) => WebFeatureSet::parse(&value)?,
                Err(std::env::VarError::NotPresent) => WebFeatureSet::defaults_for(mode),
                Err(err) => return Err(err.into()),
            },
        };

        #[cfg(not(feature = "keystore"))]
        if web_features.contains(WebFeature::JobBuilder) {
            return Err(eyre!(
                "Web feature 'job-builder' requires a build with keystore support; supported feature names in this build: advanced"
            ));
        }

        Ok(Self { mode, web_features })
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

    pub fn allows_advanced(&self) -> bool {
        self.allows_local_runtime_features() && self.web_features.contains(WebFeature::Advanced)
    }

    pub fn allows_job_builder(&self) -> bool {
        cfg!(feature = "keystore")
            && self.allows_local_runtime_features()
            && self.web_features.contains(WebFeature::JobBuilder)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WebFeature {
    Advanced,
    JobBuilder,
}

impl WebFeature {
    const ALL: [Self; 2] = [Self::Advanced, Self::JobBuilder];

    fn parse(value: &str) -> Result<Self> {
        match value {
            "advanced" => Ok(Self::Advanced),
            "job-builder" => Ok(Self::JobBuilder),
            other => Err(eyre!(
                "Invalid web feature '{other}', expected one of: {}",
                Self::known_values()
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Advanced => "advanced",
            Self::JobBuilder => "job-builder",
        }
    }

    fn known_values() -> String {
        Self::ALL
            .iter()
            .map(|feature| feature.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Clone, Debug, Default)]
pub struct WebFeatureSet {
    features: HashSet<WebFeature>,
}

impl WebFeatureSet {
    fn parse(value: &str) -> Result<Self> {
        let mut features = HashSet::new();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(Self { features });
        }

        for token in trimmed.split(',') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            features.insert(WebFeature::parse(token)?);
        }

        Ok(Self { features })
    }

    fn defaults_for(mode: RuntimeMode) -> Self {
        let mut features = HashSet::new();
        if mode == RuntimeMode::User {
            features.insert(WebFeature::Advanced);
        }
        Self { features }
    }

    fn contains(&self, feature: WebFeature) -> bool {
        self.features.contains(&feature)
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
        exporter: Exporter,
        kibana_url: String,
        runtime_mode: RuntimeMode,
    ) -> Result<(Self, std::net::SocketAddr)> {
        Self::start_with_web_features(bind_addr, port, exporter, kibana_url, runtime_mode, None).await
    }

    pub async fn start_with_web_features(
        bind_addr: [u8; 4],
        port: u16,
        mut exporter: Exporter,
        kibana_url: String,
        runtime_mode: RuntimeMode,
        web_features: Option<&str>,
    ) -> Result<(Self, std::net::SocketAddr)> {
        let (_, rx) = mpsc::channel::<(Identifiers, Bytes)>(1);
        let rx = Arc::new(RwLock::new(rx));
        let rx_clone = rx.clone();
        let docs_rx = exporter.get_docs_rx();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (stats_updates_tx, stats_updates_rx) = watch::channel(0u64);

        let (event_tx, _event_rx) = broadcast::channel::<ServerEvent>(256);
        let server_policy = ServerPolicy::with_web_features(runtime_mode, web_features)?;
        let route_policy = server_policy.clone();

        // Create shared state
        let state = Arc::new(ServerState {
            exporter: Arc::new(RwLock::new(exporter)),
            kibana_url: Arc::new(RwLock::new(kibana_url)),
            stats: Arc::new(RwLock::new(Stats::default())),
            job_requests: Arc::new(RwLock::new(HashMap::new())),
            retained_bundles: Arc::new(RwLock::new(HashMap::new())),
            shutdown: shutdown_rx,
            event_tx,
            stats_updates_tx,
            stats_updates_rx,
            runtime_mode,
            server_policy: server_policy.clone(),
            #[cfg(feature = "keystore")]
            keystore_rate_limit: Arc::new(std::sync::Mutex::new(keystore::KeystoreRateLimit::default())),
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
                .route("/documentation-outline.js", get(assets::documentation_outline))
                .route("/theme-borealis.css", get(assets::theme_borealis))
                .route("/theme", post(theme::set_theme))
                .route(
                    "/advanced/download/{token}",
                    get(bundle_download::download_retained_bundle),
                )
                .route("/docs/{*path}", get(docs::handler))
                .route("/docs", get(docs::handler_index))
                .route("/upload/process", post(file_upload::process))
                .route("/upload/submit", post(file_upload::submit))
                .route("/events", patch(events));

            let app = if route_policy.allows_advanced() {
                app.route("/advanced", get(index::advanced_page))
            } else {
                app
            };

            let app = app
                .route("/settings/modal", get(settings::get_modal))
                .route("/api/settings/update", post(settings::update_settings));

            #[cfg(feature = "keystore")]
            let app = if route_policy.allows_job_builder() {
                app.route("/jobs", get(index::jobs_page))
                    .route("/jobs/saved", get(saved_jobs::list_saved_jobs))
                    .route("/jobs/saved", post(saved_jobs::save_job))
                    .route("/jobs/saved/{name}", get(saved_jobs::load_saved_job))
                    .route("/jobs/saved/{name}", delete(saved_jobs::delete_saved_job))
            } else {
                app
            };

            #[cfg(feature = "keystore")]
            let app = if route_policy.allows_local_runtime_features() {
                app.route("/settings", get(hosts::page))
                    .route("/settings/create", post(hosts::create_host))
                    .route("/settings/update", put(hosts::update_host))
                    .route("/settings/host/{action}/{id}", post(hosts::host_action))
                    .route("/settings/cluster/{action}/{id}", post(hosts::cluster_action))
                    .route("/settings/host/upsert", post(hosts::upsert_host))
                    .route("/settings/host/delete", post(hosts::delete_host))
                    .route("/settings/secret/delete/{secret_id}", post(hosts::delete_secret_by_id))
                    .route("/settings/secret/{action}/{id}", post(hosts::secret_action))
                    .route("/settings/secret/upsert", post(hosts::upsert_secret))
                    .route("/settings/secret/delete", post(hosts::delete_secret))
                    .route("/keystore/bootstrap-modal", get(keystore::get_bootstrap_modal))
                    .route("/keystore/bootstrap", post(keystore::bootstrap))
                    .route("/keystore/modal", get(keystore::get_unlock_modal))
                    .route("/keystore/modal/process", get(keystore::get_process_unlock_modal))
                    .route("/keystore/unlock", post(keystore::unlock))
                    .route("/keystore/lock", post(keystore::lock))
                    .route("/keystore/status", get(keystore::status))
            } else {
                app
            };

            let app = if route_policy.requires_iap_headers() {
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
        tracing::info!("Starting {}-mode server on port {}", runtime_mode, bound_addr.port());
        tracing::debug!(
            "Server policy => requires_iap_headers={}, allows_local_runtime_features={}, allows_exporter_updates={}, allows_host_management={}",
            server_policy.requires_iap_headers(),
            server_policy.allows_local_runtime_features(),
            server_policy.allows_exporter_updates(),
            server_policy.allows_host_management()
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
    pub job_requests: Arc<RwLock<HashMap<u64, JobRequest>>>,
    pub retained_bundles: Arc<RwLock<HashMap<String, RetainedBundle>>>,
    pub runtime_mode: RuntimeMode,
    pub server_policy: ServerPolicy,
    #[cfg(feature = "keystore")]
    pub keystore_rate_limit: Arc<std::sync::Mutex<keystore::KeystoreRateLimit>>,
    stats: Arc<RwLock<Stats>>,
    shutdown: watch::Receiver<bool>,
    event_tx: broadcast::Sender<ServerEvent>,
    stats_updates_tx: watch::Sender<u64>,
    stats_updates_rx: watch::Receiver<u64>,
}

#[cfg(not(feature = "keystore"))]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct KeystorePageState {
    pub can_use_keystore: bool,
    pub locked: bool,
    pub lock_time: i64,
    pub show_bootstrap: bool,
}

#[cfg(not(feature = "keystore"))]
impl ServerState {
    pub(crate) async fn keystore_page_state(&self) -> KeystorePageState {
        KeystorePageState::default()
    }
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

pub(crate) fn now_epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs() as i64
}

impl ServerState {
    fn retained_bundle_signal(owner: &str, token: &str, status: &str, error: Option<&str>) -> ServerEvent {
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

    pub async fn record_job_started(&self) {
        let mut stats = self.stats.write().await;
        stats.jobs.active += 1;
        drop(stats);
        self.notify_stats_changed();
    }

    pub fn resolve_user_email(&self, headers: &HeaderMap) -> Result<(bool, String)> {
        if self.server_policy.requires_iap_headers() {
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

    pub async fn push_job_request(&self, id: u64, job: JobRequest) -> Option<JobRequest> {
        self.job_requests.write().await.insert(id, job)
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
        self.publish_event(Self::retained_bundle_signal(&owner_event, &token, "ready", None));
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

    pub async fn reject_retained_bundle(&self, token: &str, owner: &str, error: impl Into<String>, ttl: Duration) {
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
        self.publish_event(Self::retained_bundle_signal(owner, token, "error", Some(&error)));
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
            tracing::debug!("Failed to remove retained bundle {}: {}", path.display(), err);
        }
        if let Some(path) = cleanup_path
            && let Err(err) = tokio::fs::remove_dir_all(&path).await
            && err.kind() != std::io::ErrorKind::NotFound
        {
            tracing::debug!("Failed to remove retained bundle directory {}: {}", path.display(), err);
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

    pub async fn pop_job_request(&self, id: u64) -> Option<JobRequest> {
        tracing::debug!("Popping job request id: {id}");
        self.job_requests.write().await.remove(&id)
    }

    pub async fn discard_job_request(&self, id: u64) {
        if let Some(job) = self.job_requests.write().await.remove(&id) {
            job.cleanup().await;
        }
    }

    pub async fn push_key(
        &self,
        id: u64,
        identifiers: Identifiers,
        host: KnownHost,
        diagnostic_type: String,
    ) -> Option<JobRequest> {
        self.push_job_request(
            id,
            JobRequest {
                identifiers,
                input: JobInput::FromRemoteHost {
                    source: host.get_url().to_string(),
                    host,
                    diagnostic_type,
                },
            },
        )
        .await
    }

    pub async fn push_link(&self, id: u64, identifiers: Identifiers, filename: String, uri: Uri) -> Option<JobRequest> {
        tracing::debug!("Pushing service link id: {id}");
        self.push_job_request(
            id,
            JobRequest {
                identifiers,
                input: JobInput::FromServiceLink { source: filename, uri },
            },
        )
        .await
    }

    pub async fn push_upload(&self, id: u64, filename: String, path: PathBuf) -> Option<JobRequest> {
        tracing::debug!("Pushing file upload id: {id}");
        self.push_job_request(
            id,
            JobRequest {
                identifiers: Identifiers::default(),
                input: JobInput::LocalArchive {
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

    fn notify_stats_changed(&self) {
        let next = *self.stats_updates_tx.borrow() + 1;
        let _ = self.stats_updates_tx.send(next);
    }
}

pub async fn ensure_active_output_ready(state: &Arc<ServerState>) -> Result<(), String> {
    #[cfg(feature = "keystore")]
    {
        keystore::ensure_unlocked_for_active_output(state).await
    }
    #[cfg(not(feature = "keystore"))]
    {
        let _ = state;
        Ok(())
    }
}

async fn require_authenticated_user(State(state): State<Arc<ServerState>>, request: Request, next: Next) -> Response {
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
    if state.server_policy.requires_iap_headers()
        && !path_is_routable_without_iap
        && let Err(err) = state.resolve_user_email(request.headers())
    {
        tracing::warn!("Rejected unauthenticated request for {} {}: {}", method, path, err);
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
        job_requests: Arc::new(RwLock::new(HashMap::new())),
        retained_bundles: Arc::new(RwLock::new(HashMap::new())),
        runtime_mode,
        server_policy: ServerPolicy::defaults(runtime_mode),
        #[cfg(feature = "keystore")]
        keystore_rate_limit: Arc::new(std::sync::Mutex::new(keystore::KeystoreRateLimit::default())),
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
            docs: DocStats { total: 0, errors: 0 },
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
pub struct JobRunSignals {
    #[serde(default)]
    pub metadata: Identifiers,
    #[serde(default)]
    pub archive: ArchiveSignals,
    #[serde(default)]
    pub job: JobSignals,
}

#[derive(Clone, Default, Deserialize)]
pub struct KnownHostFormSignals {
    #[serde(default)]
    pub metadata: Identifiers,
    #[serde(default)]
    pub archive: ArchiveSignals,
    #[serde(default)]
    pub job: JobSignals,
}

impl From<KnownHostFormSignals> for JobRunSignals {
    fn from(signals: KnownHostFormSignals) -> Self {
        Self {
            metadata: signals.metadata,
            archive: signals.archive,
            job: signals.job,
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
    pub job: JobSignals,
    #[serde(default)]
    pub es_api: EsApiKey,
}

impl From<ApiKeyFormSignals> for JobRunSignals {
    fn from(signals: ApiKeyFormSignals) -> Self {
        Self {
            metadata: signals.metadata,
            archive: signals.archive,
            job: signals.job,
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
    pub job: JobSignals,
    #[serde(default)]
    pub service_link: ServiceLink,
}

impl From<ServiceLinkFormSignals> for JobRunSignals {
    fn from(signals: ServiceLinkFormSignals) -> Self {
        Self {
            metadata: signals.metadata,
            archive: signals.archive,
            job: signals.job,
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
    pub job: JobSignals,
    #[serde(default)]
    pub file_upload: FileUpload,
}

impl From<UploadProcessSignals> for JobRunSignals {
    fn from(signals: UploadProcessSignals) -> Self {
        Self {
            metadata: signals.metadata,
            archive: signals.archive,
            job: signals.job,
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

pub use crate::data::{CollectMode, CollectSource, JobSignals, ProcessMode, SendMode};

#[derive(Clone)]
pub struct JobRequest {
    pub identifiers: Identifiers,
    pub input: JobInput,
}

impl JobRequest {
    pub fn source(&self) -> &str {
        self.input.source()
    }

    pub async fn cleanup(&self) {
        self.input.cleanup().await;
    }
}

#[derive(Clone)]
pub enum JobInput {
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

impl JobInput {
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
                tracing::debug!("Failed to clean job input {}: {}", path.display(), err);
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
        ServerEvent::TargetedSignals { payload, .. } => PatchSignals::new(payload).write_as_axum_sse_event(),
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
        ServerEvent::ExecuteScript(script) => datastar::prelude::ExecuteScript::new(&script).write_as_axum_sse_event(),
    };

    Ok(sse_event)
}

pub fn receiver_stream(rx: mpsc::Receiver<ServerEvent>) -> impl futures::Stream<Item = Result<Event, Infallible>> {
    stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|event| (server_event_to_sse(event), rx))
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
        ServerEvent::TargetedSignals { user: target_user, .. } => target_user == user,
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
        SEC_CH_PREFERS_COLOR_SCHEME.parse().expect("valid Accept-CH value"),
    );
    headers.insert(
        CRITICAL_CH,
        SEC_CH_PREFERS_COLOR_SCHEME.parse().expect("valid Critical-CH value"),
    );
    headers.append(VARY, SEC_CH_PREFERS_COLOR_SCHEME.parse().expect("valid Vary value"));
    headers.append(VARY, "Cookie".parse().expect("valid Vary value"));

    response
}

#[cfg(test)]
mod tests {
    use super::{
        ApiKeyFormSignals, JobRunSignals, RuntimeMode, Server, ServerEvent, ServerPolicy, ServerState, Stats,
        event_visible_to_user, receiver_stream, replace_job_event, signal_event, targeted_signal_event,
        test_server_state,
    };
    #[cfg(feature = "keystore")]
    use crate::data::{
        SecretAuth, authenticate, create_keystore, resolve_secret_auth, upsert_secret_auth, write_unlock_lease,
    };
    use crate::exporter::Exporter;
    use axum::http::HeaderMap;
    use futures::StreamExt;
    #[cfg(feature = "keystore")]
    use reqwest::Client;
    use std::{collections::HashMap, sync::Arc};
    #[cfg(feature = "keystore")]
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
            job_requests: Arc::new(RwLock::new(HashMap::new())),
            retained_bundles: Arc::new(RwLock::new(HashMap::new())),
            runtime_mode: mode,
            server_policy: ServerPolicy {
                mode,
                web_features: super::WebFeatureSet::defaults_for(mode),
            },
            #[cfg(feature = "keystore")]
            keystore_rate_limit: Arc::new(std::sync::Mutex::new(super::keystore::KeystoreRateLimit::default())),
            stats: Arc::new(RwLock::new(Stats::default())),
            shutdown: watch::channel(false).1,
            event_tx: broadcast::channel(16).0,
            stats_updates_tx,
            stats_updates_rx,
        }
    }

    struct WebFeaturesEnvGuard {
        previous: Option<String>,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl Drop for WebFeaturesEnvGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => unsafe { std::env::set_var("ESDIAG_WEB_FEATURES", value) },
                None => unsafe { std::env::remove_var("ESDIAG_WEB_FEATURES") },
            }
        }
    }

    fn with_web_features_env<T>(value: Option<&str>, test: impl FnOnce() -> T) -> T {
        let env_guard = WebFeaturesEnvGuard {
            _guard: crate::test_env_lock().lock().expect("web features env lock"),
            previous: std::env::var("ESDIAG_WEB_FEATURES").ok(),
        };
        match value {
            Some(value) => unsafe { std::env::set_var("ESDIAG_WEB_FEATURES", value) },
            None => unsafe { std::env::remove_var("ESDIAG_WEB_FEATURES") },
        }

        let result = test();
        drop(env_guard);
        result
    }

    #[test]
    fn web_feature_defaults_enable_only_advanced_for_user_mode() {
        let policy = ServerPolicy {
            mode: RuntimeMode::User,
            web_features: super::WebFeatureSet::defaults_for(RuntimeMode::User),
        };

        assert!(policy.allows_advanced());
        assert!(!policy.allows_job_builder());
    }

    #[test]
    fn web_feature_defaults_disable_optional_features_for_service_mode() {
        let policy = ServerPolicy {
            mode: RuntimeMode::Service,
            web_features: super::WebFeatureSet::defaults_for(RuntimeMode::Service),
        };

        assert!(!policy.allows_advanced());
        assert!(!policy.allows_job_builder());
    }

    #[test]
    fn explicit_web_features_are_authoritative() {
        #[cfg(feature = "keystore")]
        {
            let policy =
                ServerPolicy::with_web_features(RuntimeMode::User, Some("job-builder")).expect("explicit web features");

            assert!(!policy.allows_advanced());
            assert!(policy.allows_job_builder());
        }

        #[cfg(not(feature = "keystore"))]
        {
            let err = ServerPolicy::with_web_features(RuntimeMode::User, Some("job-builder"))
                .expect_err("job-builder should require keystore support");

            assert!(err.to_string().contains("requires a build with keystore support"));
        }
    }

    #[test]
    fn env_web_features_are_used_when_cli_value_is_absent() {
        with_web_features_env(Some("job-builder"), || {
            #[cfg(feature = "keystore")]
            {
                let policy = ServerPolicy::with_web_features(RuntimeMode::User, None).expect("env web features");

                assert!(!policy.allows_advanced());
                assert!(policy.allows_job_builder());
            }

            #[cfg(not(feature = "keystore"))]
            {
                let err = ServerPolicy::with_web_features(RuntimeMode::User, None)
                    .expect_err("job-builder env should require keystore support");

                assert!(err.to_string().contains("requires a build with keystore support"));
            }
        });
    }

    #[test]
    fn empty_env_web_features_disable_optional_features() {
        with_web_features_env(Some(""), || {
            let policy = ServerPolicy::with_web_features(RuntimeMode::User, None).expect("empty env web features");

            assert!(!policy.allows_advanced());
            assert!(!policy.allows_job_builder());
        });
    }

    #[test]
    fn explicit_web_features_override_env_value() {
        with_web_features_env(Some("job-builder"), || {
            let policy =
                ServerPolicy::with_web_features(RuntimeMode::User, Some("advanced")).expect("cli overrides env");

            assert!(policy.allows_advanced());
            assert!(!policy.allows_job_builder());
        });
    }

    #[test]
    fn empty_web_features_disable_optional_features() {
        let policy = ServerPolicy::with_web_features(RuntimeMode::User, Some("  ")).expect("empty web features");

        assert!(!policy.allows_advanced());
        assert!(!policy.allows_job_builder());
    }

    #[test]
    fn unknown_web_feature_error_lists_known_values() {
        let err = ServerPolicy::with_web_features(RuntimeMode::User, Some("advanced,unknown-feature"))
            .expect_err("unknown feature should fail");

        let message = err.to_string();
        assert!(message.contains("unknown-feature"));
        assert!(message.contains("advanced"));
        assert!(message.contains("job-builder"));
    }

    #[test]
    #[cfg(not(feature = "keystore"))]
    fn job_builder_feature_requires_keystore_support() {
        let err = ServerPolicy::with_web_features(RuntimeMode::User, Some("advanced,job-builder"))
            .expect_err("job-builder should fail without keystore support");

        let message = err.to_string();
        assert!(message.contains("job-builder"));
        assert!(message.contains("requires a build with keystore support"));
        assert!(message.contains("advanced"));
    }

    #[test]
    #[cfg(feature = "keystore")]
    fn service_mode_blocks_explicit_local_web_features() {
        let policy = ServerPolicy::with_web_features(RuntimeMode::Service, Some("advanced,job-builder"))
            .expect("explicit web features");

        assert!(!policy.allows_advanced());
        assert!(!policy.allows_job_builder());
        assert!(policy.requires_iap_headers());
    }

    #[test]
    #[cfg(not(feature = "keystore"))]
    fn service_mode_rejects_unsupported_job_builder_feature() {
        let err = ServerPolicy::with_web_features(RuntimeMode::Service, Some("advanced,job-builder"))
            .expect_err("job-builder should fail without keystore support");

        assert!(err.to_string().contains("requires a build with keystore support"));
    }

    #[cfg(feature = "keystore")]
    fn setup_keystore_env() -> TempDir {
        let tmp = TempDir::new().expect("temp dir");
        let config_dir = tmp.path().join(".esdiag");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        let keystore_path = config_dir.join("secrets.yml");
        let hosts_path = config_dir.join("hosts.yml");
        unsafe {
            std::env::set_var("HOME", tmp.path());
            std::env::set_var("USERPROFILE", tmp.path());
            std::env::set_var("ESDIAG_KEYSTORE", &keystore_path);
            std::env::set_var("ESDIAG_HOSTS", &hosts_path);
            std::env::remove_var("ESDIAG_KEYSTORE_PASSWORD");
        }
        tmp
    }

    #[tokio::test]
    async fn start_with_ephemeral_port_binds_and_reports_socket() {
        let (mut server, bound_addr) = Server::start_with_web_features(
            [127, 0, 0, 1],
            0,
            Exporter::default(),
            String::new(),
            RuntimeMode::User,
            Some("advanced"),
        )
        .await
        .expect("server should bind on ephemeral port");

        assert!(bound_addr.ip().is_loopback());
        assert!(bound_addr.port() > 0);

        server.shutdown().await;
    }

    #[test]
    fn job_run_signals_deserialize_without_archive_field() {
        let payload = r#"{"metadata":{"user":"Anonymous","account":"","case_number":"","opportunity":""},"job":{"collect":{"mode":"collect","source":"known-host","known_host":"esdiag-prod","diagnostic_type":"standard","save":true,"download_dir":"/Users/reno/Downloads"},"process":{"mode":"forward","enabled":false,"product":"elasticsearch","diagnostic_type":"standard","selected":""},"send":{"mode":"remote","remote_target":"b8b9a090-21fa-419f-a731-ae8676fdd835","local_target":"directory","local_directory":"Directory /tmp/output"}}}"#;

        let parsed =
            serde_json::from_str::<JobRunSignals>(payload).expect("job run payload without archive should deserialize");
        assert_eq!(parsed.job.collect.known_host, "esdiag-prod");
        assert!(parsed.archive.download_token.is_empty());
    }

    #[test]
    fn api_key_form_signals_deserialize_without_archive_field() {
        let payload = r#"{"metadata":{"user":"Anonymous","account":"","case_number":"","opportunity":""},"job":{"collect":{"mode":"collect","source":"api-key","known_host":"","diagnostic_type":"standard","save":false,"download_dir":""},"process":{"mode":"process","enabled":true,"product":"elasticsearch","diagnostic_type":"standard","selected":""},"send":{"mode":"remote","remote_target":"","local_target":"","local_directory":""}},"es_api":{"url":"","key":"secret"}}"#;

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
        let (mut server, bound_addr) = Server::start_with_web_features(
            [127, 0, 0, 1],
            0,
            Exporter::default(),
            String::new(),
            RuntimeMode::User,
            Some("advanced"),
        )
        .await
        .expect("server should bind");
        let url = format!("http://{}/events", bound_addr);

        let client = reqwest::Client::new();
        let mut response = client.patch(url).send().await.expect("events request should succeed");
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

    #[cfg(feature = "keystore")]
    #[tokio::test]
    async fn service_mode_keystore_session_remains_disabled() {
        let state = test_state(RuntimeMode::Service);

        state.set_keystore_unlocked("pw".to_string()).await;

        assert_eq!(state.keystore_status().await, (true, 0));
        assert_eq!(state.keystore_status().await, (true, 0));
        assert_eq!(state.keystore_password().await, None);
        assert_eq!(state.keystore_password().await, None);
    }

    #[cfg(feature = "keystore")]
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn user_mode_keystore_session_is_shared_across_request_users() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        let _tmp = setup_keystore_env();

        let state = Arc::new(test_state(RuntimeMode::User));

        state.set_keystore_unlocked("pw".to_string()).await;

        assert!(state.is_keystore_unlocked().await);
        assert!(state.is_keystore_unlocked().await);
        assert_eq!(state.keystore_password().await, Some("pw".to_string()));

        state.set_keystore_locked("test").await;

        assert!(!state.is_keystore_unlocked().await);
        assert!(!state.is_keystore_unlocked().await);
    }

    #[cfg(feature = "keystore")]
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn user_mode_reads_cli_unlock_file() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        let _tmp = setup_keystore_env();
        create_keystore("pw").expect("create keystore");
        write_unlock_lease("pw", Duration::from_secs(300)).expect("write unlock lease");

        let state = Arc::new(test_state(RuntimeMode::User));

        assert!(state.is_keystore_unlocked().await);
        assert_eq!(state.keystore_password().await, Some("pw".to_string()));
    }

    #[cfg(feature = "keystore")]
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn explicit_web_lock_deletes_unlock_file() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        let _tmp = setup_keystore_env();
        create_keystore("pw").expect("create keystore");
        write_unlock_lease("pw", Duration::from_secs(300)).expect("write unlock lease");

        let state = Arc::new(test_state(RuntimeMode::User));
        assert!(state.is_keystore_unlocked().await);

        state.set_keystore_locked("manual").await;

        assert!(!state.is_keystore_unlocked().await);
        assert_eq!(state.keystore_password().await, None);
        assert!(
            !crate::data::get_unlock_path().expect("unlock path").exists(),
            "lock should delete the unlock file"
        );
    }

    #[cfg(feature = "keystore")]
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn service_mode_does_not_read_unlock_file() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        let _tmp = setup_keystore_env();
        create_keystore("pw").expect("create keystore");
        write_unlock_lease("pw", Duration::from_secs(300)).expect("write unlock lease");

        let state = Arc::new(test_state(RuntimeMode::Service));

        assert!(!state.is_keystore_unlocked().await);
        assert_eq!(state.keystore_password().await, None);
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
            "accounts.google.com:ops@example.com".parse().expect("valid header"),
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

    #[cfg(feature = "keystore")]
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn secret_delete_path_prefers_secret_id_route_over_generic_action_route() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        let _tmp = setup_keystore_env();
        authenticate("secretpw").expect("create keystore");
        upsert_secret_auth(
            "servermore",
            SecretAuth::ApiKey {
                apikey: "super-secret-api-key".to_string(),
            },
            "secretpw",
        )
        .expect("store secret");

        let (mut server, bound_addr) =
            Server::start([127, 0, 0, 1], 0, Exporter::default(), String::new(), RuntimeMode::User)
                .await
                .expect("server should bind");

        let client = Client::new();
        let unlock = client
            .post(format!("http://{bound_addr}/keystore/unlock"))
            .form(&[("password", "secretpw")])
            .send()
            .await
            .expect("unlock request should succeed");
        assert_eq!(unlock.status(), reqwest::StatusCode::NO_CONTENT);

        let delete = client
            .post(format!("http://{bound_addr}/settings/secret/delete/servermore"))
            .send()
            .await
            .expect("delete request should succeed");
        let status = delete.status();
        let body = delete.text().await.expect("delete body");

        server.shutdown().await;

        assert_eq!(status, reqwest::StatusCode::OK, "unexpected response body: {body}");
        assert!(!body.contains("id must be numeric or 'new'"));
        assert!(
            resolve_secret_auth("servermore").expect("read secret").is_none(),
            "secret should be removed by delete-by-id route"
        );
    }
}
