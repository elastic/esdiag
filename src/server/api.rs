use crate::data::diagnostic::DiagnosticReport;
use axum::{
    Router,
    extract::{DefaultBodyLimit, Multipart},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
};
use bytes::Bytes;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::{RwLock, mpsc};

// Status of a diagnostic processing job
#[derive(Clone)]
pub enum ProcessingStatus {
    Ready,
    Processing,
    Complete(DiagnosticReport),
    Error(String),
}

#[derive(Clone)]
pub struct ApiServer {
    server_handle: Option<Arc<tokio::task::JoinHandle<()>>>,
    pub rx: Option<Arc<RwLock<mpsc::Receiver<Bytes>>>>,
    //port: u16,
    state: Arc<ApiState>,
}

// Shared state for the API server
pub struct ApiState {
    upload_tx: mpsc::Sender<Bytes>,
    processing_status: Arc<RwLock<ProcessingStatus>>,
}

impl ApiServer {
    pub fn new(port: u16) -> Self {
        let (tx, rx) = mpsc::channel::<Bytes>(1);
        let rx = Arc::new(RwLock::new(rx));
        let rx_clone = rx.clone();

        // Create shared state
        let state = Arc::new(ApiState {
            upload_tx: tx.clone(),
            processing_status: Arc::new(RwLock::new(ProcessingStatus::Ready)),
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

            let app = Router::new()
                .route("/", get(Self::index_handler))
                .route("/upload", post(upload_handler))
                .route("/status", get(status_handler))
                .layer(DefaultBodyLimit::max(1024 * 1024 * 1024)); // 1 GB limit

            let addr = SocketAddr::from(([0, 0, 0, 0], port));

            // Start the server
            log::info!("Listening on port {}", port);
            match axum_server::bind(addr).serve(app.into_make_service()).await {
                Ok(_) => log::info!("Server shutdown"),
                Err(e) => log::error!("Server error: {}", e),
            }
        });

        Self {
            server_handle: Some(Arc::new(handle)),
            rx: Some(rx_clone),
            //port,
            state,
        }
    }

    // Update the processing status for a diagnostic
    pub async fn update_status(&mut self, new_status: ProcessingStatus) {
        let mut status = self.state.processing_status.write().await;
        *status = new_status;
    }

    async fn index_handler() -> impl IntoResponse {
        Html(super::index::HTML)
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

                        // Store the processing status
                        let mut status = state.processing_status.write().await;
                        *status = ProcessingStatus::Processing;

                        // Send the bytes through the channel
                        if state.upload_tx.send(bytes).await.is_ok() {
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
        let status = state.processing_status.read().await;

        match &*status {
            ProcessingStatus::Processing => (
                StatusCode::OK,
                axum::Json(serde_json::json!({
                    "status": "processing",
                    "progress": "Processing diagnostic..."
                })),
            ),
            ProcessingStatus::Complete(report) => (
                StatusCode::OK,
                axum::Json(serde_json::json!({
                    "status": "complete",
                    "report": report
                })),
            ),
            ProcessingStatus::Error(error) => (
                StatusCode::OK,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "error": error
                })),
            ),
            ProcessingStatus::Ready => (
                StatusCode::OK,
                axum::Json(serde_json::json!({
                    "status": "ready"
                })),
            ),
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(handle) = self.server_handle.take() {
            Arc::try_unwrap(handle).map(|handle| handle.abort()).ok();
        }
    }

    // Updates processing status with the diagnostic report
    pub async fn set_complete(&mut self, report: DiagnosticReport) {
        self.update_status(ProcessingStatus::Complete(report)).await;
    }

    // Updates processing status with the diagnostic report
    pub async fn set_processing(&mut self) {
        self.update_status(ProcessingStatus::Processing).await;
    }

    // Updates processing status with an error
    pub async fn set_error(&mut self, error: String) {
        self.update_status(ProcessingStatus::Error(error)).await;
    }
}

impl Default for ApiServer {
    fn default() -> Self {
        Self::new(3000)
    }
}

impl Drop for ApiServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}
