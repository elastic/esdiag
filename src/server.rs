use crate::processor::{Job, JobFailed, JobProcessing};
use axum::{
    Router,
    extract::{DefaultBodyLimit, Multipart},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
};
use bytes::Bytes;
use std::{collections::VecDeque, net::SocketAddr, sync::Arc};
use tokio::sync::{RwLock, mpsc, oneshot};

static INDEX_HTML: &str = include_str!("server/index.html");

#[derive(Clone)]
pub struct ApiServer {
    server_handle: Option<Arc<tokio::task::JoinHandle<()>>>,
    worker_handle: Option<Arc<tokio::task::JoinHandle<()>>>,
    shutdown_signal: Option<Arc<oneshot::Sender<()>>>,
    pub rx: Option<Arc<RwLock<mpsc::Receiver<(String, Bytes)>>>>,
    state: Arc<ApiState>,
}

struct ApiState {
    exporter: String,
    job: JobState,
    upload_tx: mpsc::Sender<(String, Bytes)>,
}

struct JobState {
    current: Arc<RwLock<Option<JobProcessing>>>,
    history: Arc<RwLock<Vec<Job>>>,
    queue: Arc<RwLock<VecDeque<JobProcessing>>>,
}

impl ApiServer {
    pub fn new(port: u16, exporter: String) -> Self {
        let (tx, rx) = mpsc::channel::<(String, Bytes)>(1);
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
        });

        // Start the Axum server
        let state_uploader = state.clone();
        let state_handler = state.clone();
        let handle = tokio::spawn(async move {
            // Handler closures
            let upload_handler = {
                move |multipart: Multipart| async move {
                    Self::upload_handler(multipart, state_uploader).await
                }
            };

            let status_handler =
                { move || async move { Self::status_handler(state_handler).await } };

            const ONE_GIBIBYTE: usize = 1024 * 1024 * 1024;
            let app = Router::new()
                .route("/", get(Self::index_handler))
                .route("/upload", post(upload_handler))
                .route("/status", get(status_handler))
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

    async fn index_handler() -> impl IntoResponse {
        Html(INDEX_HTML)
    }

    async fn upload_handler(mut multipart: Multipart, state: Arc<ApiState>) -> impl IntoResponse {
        // Process the multipart form
        while let Ok(Some(field)) = multipart.next_field().await {
            if field.name() == Some("file") {
                // Check if the file has a valid filename
                let file_name = match field.file_name() {
                    Some(file_name) if !file_name.ends_with(".zip") => {
                        return (
                            StatusCode::BAD_REQUEST,
                            axum::Json(serde_json::json!({
                                "error": "Invalid file type. Only .zip files are allowed."
                            })),
                        );
                    }
                    Some(file_name) => file_name.to_string(),
                    None => {
                        return (
                            StatusCode::BAD_REQUEST,
                            axum::Json(serde_json::json!({"error": "No file name provided"})),
                        );
                    }
                };
                // Get the file data
                match field.bytes().await {
                    Ok(data) => {
                        let message =
                            format!("Received upload: {} ({} bytes)", file_name, data.len());
                        log::info!("{}", message);

                        // Clone the data to avoid ownership issues
                        let bytes = Bytes::copy_from_slice(&data);

                        // Send the bytes through the channel
                        if state.upload_tx.send((file_name, bytes)).await.is_ok() {
                            return (
                                StatusCode::OK,
                                axum::Json(serde_json::json!({
                                    "status": "processing",
                                    "message": message,
                                })),
                            );
                        } else {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                axum::Json(serde_json::json!({
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
                            axum::Json(serde_json::json!({
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
            axum::Json(
                serde_json::json!({"status": "error", "error": "No file part in the request"}),
            ),
        )
    }

    async fn status_handler(state: Arc<ApiState>) -> impl IntoResponse {
        let queue_size = state.job.queue.read().await.len();
        let history = state.job.history.read().await;
        let current = state.job.current.read().await;

        match queue_size {
            0 => (
                StatusCode::OK,
                axum::Json(serde_json::json!({
                    "status": "ready",
                    "exporter": state.exporter,
                    "current": *current,
                    "queue": {
                        "size": queue_size
                    },
                    "history": *history,
                })),
            ),
            1..10 => (
                StatusCode::OK,
                axum::Json(serde_json::json!({
                    "status": "processing",
                    "progress": "Processing diagnostic...",
                    "current": *current,
                    "queue": {
                        "size": queue_size
                    },
                    "history": *history,
                })),
            ),
            _ => (
                StatusCode::OK,
                axum::Json(serde_json::json!({
                    "status": "busy",
                    "warning": "Too many jobs in queue",
                    "current": *current,
                    "queue": {
                        "size": queue_size
                    },
                    "history": *history,
                })),
            ),
        }
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
                                    log::info!("Job {} completed successfully", job_completed.id);
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
        Self::new(3000, String::new())
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
