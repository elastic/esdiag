// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

mod api;
mod api_key;
mod assets;
mod file_upload;
mod index;
mod service_link;
mod stats;
mod template;

use super::processor::Identifiers;
use crate::{client::KnownHost, data::Uri, exporter::Exporter};
use askama::Template;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::HeaderMap,
    response::sse::Event,
    routing::{get, patch, post},
};
use bytes::Bytes;
use datastar::prelude::{ElementPatchMode, PatchElements, PatchSignals};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::Infallible, net::SocketAddr, sync::Arc};
use tokio::sync::{RwLock, mpsc};

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
    stats_handle: Option<Arc<tokio::task::JoinHandle<()>>>,
    pub rx: Option<Arc<RwLock<mpsc::Receiver<(Identifiers, Bytes)>>>>,
}

impl Server {
    pub fn new(port: u16, mut exporter: Exporter, kibana_url: String) -> Self {
        let (_, rx) = mpsc::channel::<(Identifiers, Bytes)>(1);
        let rx = Arc::new(RwLock::new(rx));
        let rx_clone = rx.clone();
        let mut docs_rx = exporter.get_docs_rx();

        // Create shared state
        let state = Arc::new(ServerState {
            exporter: Arc::new(exporter),
            keys: Arc::new(RwLock::new(HashMap::new())),
            kibana_url,
            links: Arc::new(RwLock::new(HashMap::new())),
            signals: Arc::new(RwLock::new(Signals::default())),
            stats: Arc::new(RwLock::new(Stats::default())),
            uploads: Arc::new(RwLock::new(HashMap::new())),
        });

        let stats_clone = state.stats.clone();
        let stats_handle = tokio::spawn(async move {
            while let Some(docs) = docs_rx.recv().await {
                log::debug!("docs_rx: {docs}");
                stats_clone.write().await.docs.total += docs;
            }
        });

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
                .route("/upload/process", post(file_upload::process))
                .route("/upload/submit", post(file_upload::submit))
                .route("/stats", patch(stats::handler))
                .layer(DefaultBodyLimit::max(FIVE_HUNDRED_TWELVE_MEBIBYTES));

            let addr = SocketAddr::from(([0, 0, 0, 0], port));

            // Start the server
            log::info!("Listening on port {}", port);
            match axum_server::bind(addr)
                .serve(app.with_state(state).into_make_service())
                .await
            {
                Ok(_) => log::info!("Server shutdown"),
                Err(e) => log::error!("Server error: {}", e),
            }
        });

        Self {
            server_handle: Some(Arc::new(server_handle)),
            stats_handle: Some(Arc::new(stats_handle)),
            rx: Some(rx_clone),
        }
    }

    pub async fn shutdown(&mut self) {
        // Shutdown the stats thread
        if let Some(handle) = self.stats_handle.take() {
            Arc::try_unwrap(handle).map(|handle| handle.abort()).ok();
            log::debug!("Stats thread stopped");
        }
        // Shutdown the main server
        if let Some(handle) = self.server_handle.take() {
            Arc::try_unwrap(handle).map(|handle| handle.abort()).ok();
            log::debug!("Server thread stopped");
        }
    }
}

impl Default for Server {
    fn default() -> Self {
        let port = std::env::var("ESDIAG_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(2501);
        Self::new(port, Exporter::default(), String::new())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Abort the server thread if it exists
        if let Some(handle) = self.server_handle.take() {
            Arc::try_unwrap(handle).map(|handle| handle.abort()).ok();
        }

        log::info!("Server shut down");
    }
}

pub struct ServerState {
    pub exporter: Arc<Exporter>,
    pub kibana_url: String,
    pub signals: Arc<RwLock<Signals>>,
    pub uploads: Arc<RwLock<HashMap<u64, (String, Bytes)>>>,
    pub links: Arc<RwLock<HashMap<u64, (Identifiers, Uri)>>>,
    pub keys: Arc<RwLock<HashMap<u64, (Identifiers, KnownHost)>>>,
    stats: Arc<RwLock<Stats>>,
}

impl ServerState {
    pub async fn record_success(&self, _docs: u32, errors: u32) {
        let mut stats = self.stats.write().await;
        //stats.docs.total += docs as usize;
        stats.docs.errors += errors as usize;
        stats.jobs.total += 1;
        stats.jobs.success += 1;
    }

    pub async fn record_failure(&self) {
        let mut stats = self.stats.write().await;
        stats.jobs.total += 1;
        stats.jobs.failed += 1;
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
            stats: Stats::default(),
            tab: Tab::FileUpload,
        }
    }
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

fn get_user_email(headers: &HeaderMap) -> (bool, Option<String>) {
    match std::env::var("ESDIAG_USER").ok() {
        Some(user) => (false, Some(user)),
        None => {
            let has_header = headers.contains_key("X-Goog-Authenticated-User-Email");
            let email = headers
                .get("X-Goog-Authenticated-User-Email")
                .and_then(|value| value.to_str().ok())
                .map(|email| {
                    // Google auth headers are typically in format "accounts.google.com:email"
                    email.split(':').last().unwrap_or(email).to_string()
                });
            (has_header, email)
        }
    }
}
