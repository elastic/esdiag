use crate::{
    data::Uri,
    exporter::Exporter,
    processor::{Identifiers, Job, JobFailed, JobNew, JobProcessing},
    receiver::Receiver,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, net::SocketAddr, sync::Arc};
use tokio::sync::{RwLock, mpsc, oneshot};
use url::Url;

static INDEX_HTML: &str = include_str!("web/index.html");
static STYLE_CSS: &str = include_str!("web/style.css");
static SCRIPT_JS: &str = include_str!("web/script.js");
static ESDIAG_SVG: &str = include_str!("web/esdiag.svg");

#[derive(Debug, Deserialize, Serialize)]
struct UploadServiceRequest {
    metadata: Identifiers,
    token: String,
    url: String,
}

impl From<UploadServiceRequest> for Identifiers {
    fn from(request: UploadServiceRequest) -> Self {
        Identifiers {
            account: request.metadata.account.clone(),
            case_number: request.metadata.case_number,
            filename: request.metadata.filename.clone(),
            opportunity: None,
            user: request.metadata.user,
        }
    }
}

#[derive(Clone)]
pub struct ApiServer {
    server_handle: Option<Arc<tokio::task::JoinHandle<()>>>,
    worker_handle: Option<Arc<tokio::task::JoinHandle<()>>>,
    shutdown_signal: Option<Arc<oneshot::Sender<()>>>,
    pub rx: Option<Arc<RwLock<mpsc::Receiver<(Identifiers, Bytes)>>>>,
    state: Arc<ApiState>,
}

struct ApiState {
    exporter: String,
    kibana: String,
    job: JobState,
    upload_tx: mpsc::Sender<(Identifiers, Bytes)>,
}

struct JobState {
    current: Arc<RwLock<Option<JobProcessing>>>,
    history: Arc<RwLock<Vec<Job>>>,
    queue: Arc<RwLock<VecDeque<JobProcessing>>>,
}

impl ApiServer {
    pub fn new(port: u16, exporter: String, kibana: String) -> Self {
        let (tx, rx) = mpsc::channel::<(Identifiers, Bytes)>(1);
        let rx = Arc::new(RwLock::new(rx));
        let rx_clone = rx.clone();

        // Create shared state
        let state = Arc::new(ApiState {
            upload_tx: tx.clone(),
            job: JobState {
                current: Arc::new(RwLock::new(None)),
                history: Arc::new(RwLock::new(Vec::with_capacity(100))),
                queue: Arc::new(RwLock::new(VecDeque::with_capacity(10))),
            },
            exporter,
            kibana,
        });

        // Start the Axum server
        let state_uploader = state.clone();
        let state_status = state.clone();
        let state_upload_service = state.clone();
        let handle = tokio::spawn(async move {
            // Handler closures
            let upload_handler = {
                move |headers, multipart: Multipart| async move {
                    Self::upload_handler(headers, multipart, state_uploader).await
                }
            };

            let status_handler =
                { move |headers| async move { Self::status_handler(headers, state_status).await } };

            let upload_service_handler = {
                move |headers, json: Json<UploadServiceRequest>| async move {
                    Self::upload_service_handler(headers, json, state_upload_service).await
                }
            };

            const ONE_GIBIBYTE: usize = 1024 * 1024 * 1024;
            let app = Router::new()
                .route("/", get(Self::index_handler))
                .route("/style.css", get(Self::style_handler))
                .route("/script.js", get(Self::script_handler))
                .route("/favicon.ico", get(Self::logo_handler))
                .route("/esdiag.svg", get(Self::logo_handler))
                .route("/upload", post(upload_handler))
                .route("/status", get(status_handler))
                .route("/upload_service", post(upload_service_handler))
                .layer(DefaultBodyLimit::max(ONE_GIBIBYTE));

            let addr = SocketAddr::from(([0, 0, 0, 0], port));

            // Start the server
            log::info!("Listening on port {}", port);
            match axum_server::bind(addr).serve(app.into_make_service()).await {
                Ok(_) => log::info!("Server shutdown"),
                Err(e) => log::error!("Server error: {}", e),
            }
        });

        let mut server = Self {
            server_handle: Some(Arc::new(handle)),
            worker_handle: None,
            shutdown_signal: None,
            rx: Some(rx_clone),
            state,
        };

        server.start_worker();
        server
    }

    async fn logo_handler() -> impl IntoResponse {
        (
            StatusCode::OK,
            [("Content-Type", "image/svg+xml")],
            ESDIAG_SVG,
        )
    }

    async fn index_handler() -> impl IntoResponse {
        Html(INDEX_HTML)
    }

    async fn script_handler() -> impl IntoResponse {
        (
            StatusCode::OK,
            [("Content-Type", "application/javascript")],
            SCRIPT_JS,
        )
    }

    async fn style_handler() -> impl IntoResponse {
        (StatusCode::OK, [("Content-Type", "text/css")], STYLE_CSS)
    }

    async fn upload_handler(
        headers: HeaderMap,
        mut multipart: Multipart,
        state: Arc<ApiState>,
    ) -> impl IntoResponse {
        // Extract authenticated user email from header
        let username = headers
            .get("X-Goog-Authenticated-User-Email")
            .and_then(|value| value.to_str().ok())
            .map(|email| {
                // Google auth headers are typically in format "accounts.google.com:email"
                email.split(':').last().unwrap_or(email).to_string()
            });

        // Process the multipart form
        while let Ok(Some(field)) = multipart.next_field().await {
            if field.name() == Some("file") {
                // Check if the file has a valid filename
                let filename = match field.file_name() {
                    Some(filename) if !filename.ends_with(".zip") => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(serde_json::json!({
                                "error": "Invalid file type. Only .zip files are allowed."
                            })),
                        );
                    }
                    Some(file_name) => file_name.to_string(),
                    None => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(serde_json::json!({"error": "No file name provided"})),
                        );
                    }
                };
                // Get the file data
                match field.bytes().await {
                    Ok(data) => {
                        let message =
                            format!("Received upload: {} ({} bytes)", filename, data.len());
                        log::info!("{}", message);

                        // Clone the data to avoid ownership issues
                        let bytes = Bytes::copy_from_slice(&data);
                        let identifiers = Identifiers {
                            account: None,
                            case_number: None,
                            filename: Some(filename),
                            user: username,
                            opportunity: None,
                        };

                        // Send the bytes through the channel
                        if state.upload_tx.send((identifiers, bytes)).await.is_ok() {
                            return (
                                StatusCode::OK,
                                Json(serde_json::json!({
                                    "status": "processing",
                                    "message": message,
                                })),
                            );
                        } else {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(serde_json::json!({
                                    "status": "error",
                                    "error": "Failed to process the upload"
                                })),
                            );
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to read upload data: {}", e);
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({
                                "status": "error",
                                "error": format!("Failed to read upload data: {}", e)
                            })),
                        );
                    }
                }
            }
        }

        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"status": "error", "error": "No file part in the request"})),
        )
    }

    async fn status_handler(headers: HeaderMap, state: Arc<ApiState>) -> impl IntoResponse {
        // Extract authenticated user email from header
        let user_email = headers
            .get("X-Goog-Authenticated-User-Email")
            .and_then(|value| value.to_str().ok())
            .map(|email| {
                // Google auth headers are typically in format "accounts.google.com:email"
                email.split(':').last().unwrap_or(email).to_string()
            });

        let queue_size = state.job.queue.read().await.len();
        let current = state.job.current.read().await;
        let history = state.job.history.read().await;
        let history = history
            .iter()
            .filter(|entry| entry.user() == user_email)
            .collect::<Vec<&Job>>();

        match queue_size {
            0 => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "ready",
                    "exporter": state.exporter,
                    "kibana": state.kibana,
                    "user": user_email,
                    "current": *current,
                    "queue": {
                        "size": queue_size
                    },
                    "history": history,
                })),
            ),
            1..10 => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "processing",
                    "progress": "Processing diagnostic...",
                    "kibana": state.kibana,
                    "user": user_email,
                    "current": *current,
                    "queue": {
                        "size": queue_size
                    },
                    "history": *history,
                })),
            ),
            _ => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "busy",
                    "warning": "Too many jobs in queue",
                    "kibana": state.kibana,
                    "user": user_email,
                    "current": *current,
                    "queue": {
                        "size": queue_size
                    },
                    "history": *history,
                })),
            ),
        }
    }

    async fn upload_service_handler(
        headers: HeaderMap,
        Json(payload): Json<UploadServiceRequest>,
        state: Arc<ApiState>,
    ) -> impl IntoResponse {
        log::info!("Received elastic uploader request for: {}", payload.url);

        // Extract authenticated user email from header
        let username = headers
            .get("X-Goog-Authenticated-User-Email")
            .and_then(|value| value.to_str().ok())
            .map(|email| {
                // Google auth headers are typically in format "accounts.google.com:email"
                email.split(':').last().unwrap_or(email).to_string()
            });

        // Construct the URL with token authentication
        let uploader_service_url = match Url::parse(&payload.url) {
            Ok(mut url) => {
                // Set username to "token" and password to the actual token
                if url.set_username("token").is_err() {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": "Failed to set token in URL"
                        })),
                    );
                }
                if url.set_password(Some(&payload.token)).is_err() {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": "Failed to set token in URL"
                        })),
                    );
                }
                url
            }
            Err(e) => {
                log::error!("Invalid URL provided: {}", e);
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Invalid URL: {}", e)
                    })),
                );
            }
        };

        // Create URI from the URL
        let uri = match Uri::try_from(uploader_service_url.to_string()) {
            Ok(uri) => uri,
            Err(e) => {
                log::error!("Failed to create URI: {}", e);
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Failed to create URI: {}", e)
                    })),
                );
            }
        };

        // Create receiver from URI
        let receiver = match Receiver::try_from(uri) {
            Ok(receiver) => receiver,
            Err(e) => {
                log::error!("Failed to create receiver: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": format!("Failed to create receiver: {}", e)
                    })),
                );
            }
        };

        let identifiers = Identifiers::from(payload).default_user(username.as_ref());
        let exporter = match Exporter::try_from(None) {
            Ok(exporter) => exporter.with_identifiers(identifiers),
            Err(e) => {
                log::error!("Failed to create exporter: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": format!("Failed to create exporter: {}", e)
                    })),
                );
            }
        };

        let job = JobNew::new(&exporter.identifiers(), receiver);
        let job_id = job.id.clone();

        let job_ready = match job.ready(exporter).await {
            Ok(job_ready) => job_ready,
            Err(failed_job) => {
                log::error!("Failed to prepare job: {}", failed_job.error);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": format!("Failed to prepare job: {}", failed_job.error)
                    })),
                );
            }
        };

        // Start processing
        let job_processing = job_ready.start();

        // Add to queue
        let mut queue = state.job.queue.write().await;
        queue.push_back(job_processing);
        let queue_size = queue.len();

        log::info!("Added elastic uploader job to queue (size: {})", queue_size);

        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "processing",
                "job_id": job_id,
                "queue_size": queue_size,
            })),
        )
    }

    pub async fn shutdown(&mut self) {
        // Send shutdown signal to worker thread if it exists
        if let Some(tx) = self.shutdown_signal.take() {
            log::debug!("Sending shutdown signal to worker thread");
            if let Err(e) = Arc::try_unwrap(tx).map(|tx| tx.send(())) {
                log::warn!("Failed to send shutdown signal to worker thread: {:?}", e);
            }
        }

        // Wait for worker thread to complete if it exists
        if let Some(handle) = self.worker_handle.take() {
            log::debug!("Waiting for worker thread to complete");

            // Use a timeout to avoid waiting forever
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                Arc::try_unwrap(handle).unwrap(),
            )
            .await
            {
                Ok(result) => match result {
                    Ok(_) => log::info!("Worker thread shut down successfully"),
                    Err(e) => log::warn!("Error joining worker thread: {:?}", e),
                },
                Err(_) => log::warn!("Timeout waiting for worker thread to shut down"),
            }
        }

        // Shutdown the main server
        if let Some(handle) = self.server_handle.take() {
            Arc::try_unwrap(handle).map(|handle| handle.abort()).ok();
            log::debug!("Server thread aborted");
        }
    }

    pub async fn job_push(&mut self, job: JobProcessing) {
        let mut queue = self.state.job.queue.write().await;
        log::debug!("Adding job {} to queue {}", job.id, queue.len());
        queue.push_back(job);
    }

    pub async fn job_record_failure(&mut self, job: JobFailed) {
        log::error!("Job {} failed with error: {}", job.id, job.error);
        self.state.job.history.write().await.push(Job::Failed(job));
    }

    // Start a thread to process diagnostics in the background
    fn start_worker(&mut self) {
        let state = self.state.clone();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        self.shutdown_signal = Some(Arc::new(shutdown_tx));

        let handle = tokio::spawn(async move {
            log::info!("Starting diagnostic worker thread");

            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        log::debug!("Worker thread received shutdown signal");
                        break;
                    }
                    _ = async {
                        // Check if there are any jobs in the queue
                        let mut queue = state.job.queue.write().await;
                        if let Some(job) = queue.pop_front() {
                            // Release the lock before processing
                            drop(queue);

                            let mut current = state.job.current.write().await;
                            *current = Some(job.clone());
                            drop(current);

                            log::info!("Processing job {} from queue", job.id);
                            match job.process().await {
                                Ok(job_completed) => {
                                    log::info!("Job {} completed in {:.3} seconds", job_completed.id, job_completed.processing_seconds());
                                    let mut history = state.job.history.write().await;
                                    history.push(Job::Completed(job_completed));
                                    let mut current = state.job.current.write().await;
                                    *current = None;
                                }
                                Err(job_failed) => {
                                    log::error!(
                                        "Job {} failed with error: {}",
                                        job_failed.id,
                                        job_failed.error
                                    );
                                    let mut history = state.job.history.write().await;
                                    history.push(Job::Failed(job_failed));
                                    let mut current = state.job.current.write().await;
                                    *current = None;
                                }
                            }
                        } else {
                            // No jobs in queue, sleep for a while before checking again
                            drop(queue);
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                    } => {}
                }
            }
        });

        self.worker_handle = Some(Arc::new(handle));
        log::debug!("Diagnostic worker thread started");
    }
}

impl Default for ApiServer {
    fn default() -> Self {
        Self::new(3000, String::new(), String::new())
    }
}

impl Drop for ApiServer {
    fn drop(&mut self) {
        // Abort the server thread if it exists
        if let Some(handle) = self.server_handle.take() {
            Arc::try_unwrap(handle).map(|handle| handle.abort()).ok();
        }

        // Send shutdown signal to worker thread if it exists
        if let Some(tx) = self.shutdown_signal.take() {
            if let Err(e) = Arc::try_unwrap(tx).map(|tx| tx.send(())) {
                log::warn!("Failed to send shutdown signal to worker thread: {:?}", e);
            }
        }

        log::info!("ApiServer dropped, server and worker threads are being shut down");
    }
}
