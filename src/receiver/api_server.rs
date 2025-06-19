use super::{Receive, ReceiveMultiple, ReceiveRaw, archive::trim_to_working_directory};
use crate::data::diagnostic::{DataSource, DiagnosticReport, data_source::PathType};

use axum::extract::{DefaultBodyLimit, Multipart as AxumMultipart};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::{
    Router,
    extract::Path,
    routing::{get, post},
};
use bytes::Bytes;
use eyre::{Result, eyre};
use serde::de::DeserializeOwned;
use std::{
    collections::HashMap,
    io::{BufReader, Cursor},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::{RwLock, mpsc, oneshot};
use zip::ZipArchive;

type ArchiveCursor = ZipArchive<BufReader<Cursor<Bytes>>>;
type ArchivePointer = Arc<RwLock<Option<ArchiveCursor>>>;

// Status of a diagnostic processing job
#[derive(Clone)]
pub enum ProcessingStatus {
    Processing,
    Complete(DiagnosticReport),
    Failed(String),
}

// Shared state for the API server
pub struct ApiState {
    upload_tx: mpsc::Sender<Bytes>,
    processing_status: Arc<RwLock<HashMap<String, ProcessingStatus>>>,
    current_upload_id: Arc<RwLock<Option<String>>>,
    report_tx: Arc<RwLock<Option<oneshot::Sender<DiagnosticReport>>>>,
}

#[derive(Clone)]
pub struct ApiReceiver {
    archive: ArchivePointer,
    subdir: Option<PathBuf>,
    server_handle: Option<Arc<tokio::task::JoinHandle<()>>>,
    rx: Option<Arc<RwLock<mpsc::Receiver<Bytes>>>>,
    port: u16,
    state: Arc<ApiState>,
}

impl ApiReceiver {
    pub fn new(port: u16) -> Self {
        let (tx, rx) = mpsc::channel::<Bytes>(1);
        let rx = Arc::new(RwLock::new(rx));
        let rx_clone = rx.clone();

        // Create shared state
        let state = Arc::new(ApiState {
            upload_tx: tx.clone(),
            processing_status: Arc::new(RwLock::new(HashMap::new())),
            current_upload_id: Arc::new(RwLock::new(None)),
            report_tx: Arc::new(RwLock::new(None)),
        });

        // Start the Axum server
        let state_clone = state.clone();
        let handle = tokio::spawn(async move {
            // Handler closures
            let upload_handler = {
                let state = state_clone.clone();
                move |multipart: AxumMultipart| {
                    let state = state.clone();
                    async move { Self::upload_handler(multipart, state).await }
                }
            };

            let status_handler = {
                let state = state_clone.clone();
                move |Path(id): Path<String>| {
                    let state = state.clone();
                    async move { Self::status_handler(id, state).await }
                }
            };

            let app = Router::new()
                .route("/", get(Self::index_handler))
                .route("/upload", post(upload_handler))
                .route("/status/{id}", get(status_handler))
                .layer(DefaultBodyLimit::max(1024 * 1024 * 50)); // 50 MB limit

            let addr = SocketAddr::from(([0, 0, 0, 0], port));

            // Start the server
            log::info!("Listening on port {}", port);
            match axum_server::bind(addr).serve(app.into_make_service()).await {
                Ok(_) => log::info!("Server shutdown"),
                Err(e) => log::error!("Server error: {}", e),
            }
        });

        Self {
            archive: Arc::new(RwLock::new(None)),
            subdir: None,
            server_handle: Some(Arc::new(handle)),
            rx: Some(rx_clone),
            port,
            state,
        }
    }

    // Set the report channel
    pub async fn set_report_receiver(&mut self, report_tx: oneshot::Sender<DiagnosticReport>) {
        let mut tx_lock = self.state.report_tx.write().await;
        *tx_lock = Some(report_tx);
    }

    // Update the processing status for a diagnostic
    pub async fn update_status(&self, id: &str, status: ProcessingStatus) {
        let mut status_map = self.state.processing_status.write().await;
        status_map.insert(id.to_string(), status.clone());

        // If this is the current upload and it's complete, send the report
        if let Some(current_id) = self.state.current_upload_id.read().await.as_ref() {
            if current_id == id {
                if let ProcessingStatus::Complete(report) = status.clone() {
                    let mut tx_lock = self.state.report_tx.write().await;
                    if let Some(tx) = tx_lock.take() {
                        let _ = tx.send(report);
                    }
                }
            }
        }
    }

    async fn index_handler() -> impl IntoResponse {
        Html(
            r#"
            <!DOCTYPE html>
            <html>
                <head>
                    <title>Elasticsearch Diagnostics Upload</title>
                    <style>
                        body {
                            font-family: Arial, sans-serif;
                            max-width: 600px;
                            margin: 0 auto;
                            padding: 20px;
                        }
                        h1 {
                            color: #005571;
                        }
                        .upload-form {
                            border: 1px solid #ddd;
                            padding: 20px;
                            border-radius: 5px;
                            background-color: #f9f9f9;
                        }
                        .button {
                            background-color: #005571;
                            color: white;
                            padding: 10px 15px;
                            border: none;
                            border-radius: 4px;
                            cursor: pointer;
                            margin-top: 10px;
                        }
                        .button:hover {
                            background-color: #00435a;
                        }
                        #status-container {
                            margin-top: 20px;
                            padding: 15px;
                            border-radius: 5px;
                            display: none;
                        }
                        .success {
                            background-color: #e6f4ea;
                            border: 1px solid #34a853;
                        }
                        .error {
                            background-color: #fce8e6;
                            border: 1px solid #ea4335;
                        }
                        .processing {
                            background-color: #e8f0fe;
                            border: 1px solid #4285f4;
                        }
                        .spinner {
                            display: inline-block;
                            width: 20px;
                            height: 20px;
                            border: 3px solid rgba(0, 85, 113, 0.3);
                            border-radius: 50%;
                            border-top-color: #005571;
                            animation: spin 1s ease-in-out infinite;
                            margin-right: 10px;
                            vertical-align: middle;
                        }
                        /* Specific styling for spinner in processing state */
                        .processing .spinner {
                            border: 3px solid rgba(66, 133, 244, 0.3);
                            border-top-color: #4285f4;
                        }
                        @keyframes spin {
                            to { transform: rotate(360deg); }
                        }
                        .hidden {
                            display: none;
                        }
                        .diagnostic-info {
                            margin-top: 10px;
                            font-weight: bold;
                        }
                        .processing-status {
                            margin-top: 5px;
                            font-style: italic;
                        }
                    </style>
                </head>
                <body>
                    <h1>Elasticsearch Diagnostics Upload</h1>
                    <div class="upload-form">
                        <form id="upload-form" action="/upload" method="post" enctype="multipart/form-data">
                            <p>Select a diagnostic bundle (ZIP file):</p>
                            <input type="file" name="file" accept=".zip" required>
                            <br>
                            <input type="submit" value="Upload" class="button" id="upload-button">
                        </form>
                    </div>
                    <div id="status-container"></div>

                    <script>
                        let processingId = null;
                        let pollingInterval = null;

                        // Function to poll for processing status
                        function pollProcessingStatus(id) {
                            return fetch(`/status/${id}`)
                                .then(response => {
                                    if (!response.ok) {
                                        throw new Error(`HTTP error ${response.status}`);
                                    }
                                    return response.json();
                                })
                                .then(data => {
                                    const statusContainer = document.getElementById('status-container');

                                    switch (data.status) {
                                        case 'processing':
                                            // Show processing status in the status container
                                            statusContainer.style.display = 'block';
                                            // First clear all classes then add processing
                                            statusContainer.className = '';
                                            statusContainer.classList.add('processing');
                                            // Set new HTML content with spinner
                                            statusContainer.innerHTML = '';  // First clear all content
                                            statusContainer.innerHTML = `
                                                <div class="spinner"></div>
                                                <span>Processing diagnostic bundle (${data.progress || 'in progress'})...</span>
                                            `;
                                            return false; // continue polling

                                        case 'complete':
                                            // Enable upload button
                                            document.getElementById('upload-button').disabled = false;

                                            // Show success message - completely replace content
                                            statusContainer.style.display = 'block';
                                            statusContainer.className = '';
                                            statusContainer.classList.add('success');

                                            // Display diagnostic ID and report information - no spinner
                                            statusContainer.innerHTML = '';  // First clear all content
                                            let message = `<p>Diagnostic processing complete!</p>`;
                                            if (data.report) {
                                                message += `<p class="diagnostic-info">Diagnostic ID: ${data.report.metadata.id}</p>`;
                                                message += `<p>Created ${data.report.docs.created} documents for ${data.report.product} diagnostic</p>`;
                                            }
                                            statusContainer.innerHTML = message;
                                            return true; // stop polling

                                        case 'failed':
                                            // Enable upload button
                                            document.getElementById('upload-button').disabled = false;

                                            // Show error message - completely replace content
                                            statusContainer.style.display = 'block';
                                            statusContainer.className = '';
                                            statusContainer.classList.add('error');
                                            // Make sure we completely replace the content
                                            statusContainer.innerHTML = '';  // First clear all content
                                            statusContainer.innerHTML = `<p>Error: ${data.error || 'Processing failed'}</p>`;
                                            return true; // stop polling

                                        default:
                                            return false; // continue polling on unknown status
                                    }
                                })
                                .catch(error => {
                                    console.error('Polling error:', error);
                                    return false; // continue polling on error
                                });
                        }

                        document.getElementById('upload-form').addEventListener('submit', function(e) {
                            e.preventDefault();

                            // Disable upload button
                            document.getElementById('upload-button').disabled = true;

                            // Show status with spinner for upload
                            const statusContainer = document.getElementById('status-container');
                            statusContainer.innerHTML = '';  // First clear all content
                            statusContainer.innerHTML = `
                                <div class="spinner"></div>
                                <span>Uploading diagnostic bundle...</span>
                            `;
                            statusContainer.className = '';
                            statusContainer.classList.add('processing');
                            statusContainer.style.display = 'block';

                            // Clear any existing polling
                            if (pollingInterval) {
                                clearInterval(pollingInterval);
                            }

                            // Get form data
                            const formData = new FormData(this);

                            // Send the upload request
                            fetch('/upload', {
                                method: 'POST',
                                body: formData
                            })
                            .then(response => {
                                if (!response.ok) {
                                    throw new Error(`HTTP error ${response.status}`);
                                }
                                return response.json();
                            })
                            .then(data => {
                                // Update status container to show processing
                                statusContainer.innerHTML = '';  // First clear all content
                                statusContainer.innerHTML = `
                                    <div class="spinner"></div>
                                    <span>Processing diagnostic bundle...</span>
                                `;

                                // Start polling for status
                                if (data.processingId) {
                                    processingId = data.processingId;

                                    pollingInterval = setInterval(() => {
                                        pollProcessingStatus(processingId).then(shouldStop => {
                                            if (shouldStop) {
                                                clearInterval(pollingInterval);
                                            }
                                        });
                                    }, 1000);
                                } else {
                                    // Fallback if no processing ID provided
                                    document.getElementById('upload-button').disabled = false;

                                    // Display success without spinner
                                    statusContainer.style.display = 'block';
                                    statusContainer.className = '';
                                    statusContainer.classList.add('success');
                                    // Completely replace HTML content
                                    statusContainer.innerHTML = '';  // First clear all content
                                    statusContainer.innerHTML = `<p>${data.message || 'Upload successful'}</p>`;
                                }
                            })
                            .catch(error => {
                                // Enable upload button
                                document.getElementById('upload-button').disabled = false;

                                // Show error message - clear everything
                                statusContainer.style.display = 'block';
                                statusContainer.className = '';
                                statusContainer.classList.add('error');
                                // Completely replace HTML content
                                statusContainer.innerHTML = '';  // First clear all content
                                statusContainer.innerHTML = `<p>Error: ${error.message}</p>`;
                            });
                        });
                    </script>
                </body>
            </html>
            "#,
        )
    }

    async fn upload_handler(
        mut multipart: AxumMultipart,
        state: Arc<ApiState>,
    ) -> impl IntoResponse {
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
                            format!("Received ZIP file: {} ({} bytes)", file_name, data.len());
                        log::info!("{}", message);

                        // Clone the data to avoid ownership issues
                        let bytes = Bytes::copy_from_slice(&data);

                        // Generate a unique processing ID
                        let processing_id = uuid::Uuid::new_v4().to_string();

                        // Store the processing status
                        let mut status_map = state.processing_status.write().await;
                        status_map.insert(processing_id.clone(), ProcessingStatus::Processing);

                        // Set as current upload
                        *state.current_upload_id.write().await = Some(processing_id.clone());

                        // Send the bytes through the channel
                        if state.upload_tx.send(bytes).await.is_ok() {
                            return (
                                StatusCode::OK,
                                axum::Json(serde_json::json!({
                                    "message": message,
                                    "processingId": processing_id
                                })),
                            );
                        } else {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                axum::Json(serde_json::json!({
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
                                "error": format!("Failed to read upload data: {}", e)
                            })),
                        );
                    }
                }
            }
        }

        (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": "No file part in the request"})),
        )
    }

    async fn status_handler(id: String, state: Arc<ApiState>) -> impl IntoResponse {
        let status_map = state.processing_status.read().await;

        if let Some(status) = status_map.get(&id) {
            match status {
                ProcessingStatus::Processing => (
                    StatusCode::OK,
                    axum::Json(serde_json::json!({
                        "status": "processing",
                        "progress": "analyzing diagnostic data"
                    })),
                ),
                ProcessingStatus::Complete(report) => (
                    StatusCode::OK,
                    axum::Json(serde_json::json!({
                        "status": "complete",
                        "report": report
                    })),
                ),
                ProcessingStatus::Failed(error) => (
                    StatusCode::OK,
                    axum::Json(serde_json::json!({
                        "status": "failed",
                        "error": error
                    })),
                ),
            }
        } else {
            (
                StatusCode::NOT_FOUND,
                axum::Json(serde_json::json!({
                    "error": "Processing ID not found"
                })),
            )
        }
    }

    fn resolve_archive_path(&self, archive: &mut ArchiveCursor, filename: &str) -> Result<String> {
        let full_path = match &self.subdir {
            // Ugly hack to make ECK bundles with double-slashed paths work
            // This will break if the sub-paths are fixed in the ECK bundles
            Some(subdir) => format!("{}//{}", subdir.display(), filename),
            None => {
                let mut path = PathBuf::from(archive.by_index(0)?.name().to_string());
                trim_to_working_directory(&mut path);
                let path = path.join(filename);
                format!("{}", path.display())
            }
        };
        Ok(full_path)
    }

    pub fn shutdown(&mut self) {
        if let Some(handle) = self.server_handle.take() {
            Arc::try_unwrap(handle).map(|handle| handle.abort()).ok();
        }
    }

    pub fn has_archive(&self) -> bool {
        self.archive.blocking_read().is_some()
    }

    pub fn clear_archive(&mut self) {
        self.archive = Arc::new(RwLock::new(None));
    }

    // Updates processing status with the diagnostic report
    pub async fn set_complete(&self, report: DiagnosticReport) {
        if let Some(id) = self.state.current_upload_id.read().await.clone() {
            self.update_status(&id, ProcessingStatus::Complete(report))
                .await;
        }
    }

    // Updates processing status with an error
    pub async fn set_failed(&self, error: String) {
        if let Some(id) = self.state.current_upload_id.read().await.clone() {
            self.update_status(&id, ProcessingStatus::Failed(error))
                .await;
        }
    }
}

impl Default for ApiReceiver {
    fn default() -> Self {
        Self::new(3000)
    }
}

impl Drop for ApiReceiver {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Implementation of the Receive trait for the REST API receiver
impl Receive for ApiReceiver {
    async fn is_connected(&self) -> bool {
        // If we have a server handle, consider it connected
        self.server_handle.is_some()
    }

    async fn collection_date(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }

    /// Read the type's file from the in-memory archive
    async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        let mut archive_lock = self.archive.write().await;

        // We are using interior mutability here to cache the archive to memory on first access
        if archive_lock.is_none() {
            // Try to receive the uploaded archive if it's not already loaded
            if let Some(rx) = &self.rx {
                let mut rx_lock = rx.write().await;
                match rx_lock.recv().await {
                    Some(bytes) => {
                        log::info!("Received archive size: {} bytes", bytes.len());
                        let cursor = BufReader::new(Cursor::new(bytes));
                        match ZipArchive::new(cursor) {
                            Ok(archive) => {
                                archive_lock.replace(archive);
                            }
                            Err(e) => {
                                return Err(eyre!("Failed to open ZIP archive: {}", e));
                            }
                        }
                    }
                    None => {
                        return Err(eyre!("No archive has been uploaded yet"));
                    }
                }
            } else {
                return Err(eyre!("Receiver channel not initialized"));
            }
        }

        // Early return if archive is not available
        let Some(archive) = archive_lock.as_mut() else {
            return Err(eyre!("Archive was not uploaded or cached"));
        };

        // Determine the fully-qualified filename within in the archive
        let filename = self.resolve_archive_path(archive, T::source(PathType::File)?)?;

        // Read and deserialize the file from the archive
        log::debug!("Reading {} from uploaded archive", filename);
        let file = match archive.by_name(&filename) {
            Ok(file) => file,
            Err(_) => return Err(eyre!("Failed to read file {} from archive", filename)),
        };
        let reader = BufReader::new(file);
        let data: T = serde_json::from_reader(reader)?;
        Ok(data)
    }
}

impl ReceiveRaw for ApiReceiver {
    async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        let mut archive_lock = self.archive.write().await;

        // We are using interior mutability here to cache the archive to memory on first access
        if archive_lock.is_none() {
            // Try to receive the uploaded archive if it's not already loaded
            if let Some(rx) = &self.rx {
                let mut rx_lock = rx.write().await;
                match rx_lock.recv().await {
                    Some(bytes) => {
                        log::info!("Received archive size: {} bytes", bytes.len());
                        let cursor = BufReader::new(Cursor::new(bytes));
                        match ZipArchive::new(cursor) {
                            Ok(archive) => {
                                archive_lock.replace(archive);
                            }
                            Err(e) => {
                                return Err(eyre!("Failed to open ZIP archive: {}", e));
                            }
                        }
                    }
                    None => {
                        return Err(eyre!("No archive has been uploaded yet"));
                    }
                }
            } else {
                return Err(eyre!("Receiver channel not initialized"));
            }
        }

        // Early return if archive is not available
        let Some(archive) = archive_lock.as_mut() else {
            return Err(eyre!("Archive was not uploaded or cached"));
        };

        // Determine the fully-qualified filename within in the archive
        let filename = self.resolve_archive_path(archive, T::source(PathType::File)?)?;

        // Read the file as a string from the archive
        log::debug!("Reading raw {} from uploaded archive", filename);
        let mut file = match archive.by_name(&filename) {
            Ok(file) => file,
            Err(_) => return Err(eyre!("Failed to read file {} from archive", filename)),
        };
        let mut content = String::new();
        std::io::Read::read_to_string(&mut file, &mut content)?;
        Ok(content)
    }
}

impl ReceiveMultiple for ApiReceiver {
    fn set_work_dir(&mut self, work_dir: &str) -> Result<()> {
        log::trace!("Setting subdir: {}", work_dir);
        self.subdir = Some(PathBuf::from(work_dir));
        Ok(())
    }
}

impl std::fmt::Display for ApiReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "REST API Server (port {})", self.port)
    }
}
