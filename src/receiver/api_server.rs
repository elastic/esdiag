use super::{Receive, ReceiveMultiple, ReceiveRaw, archive::trim_to_working_directory};
use crate::data::diagnostic::{DataSource, data_source::PathType};

use axum::extract::{DefaultBodyLimit, Multipart as AxumMultipart};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::{
    Router,
    routing::{get, post},
};
use bytes::Bytes;
use eyre::{Result, eyre};
use serde::de::DeserializeOwned;
use std::{
    io::{BufReader, Cursor},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::{RwLock, mpsc};
use zip::ZipArchive;

type ArchiveCursor = ZipArchive<BufReader<Cursor<Bytes>>>;
type ArchivePointer = Arc<RwLock<Option<ArchiveCursor>>>;

#[derive(Clone)]
pub struct ApiReceiver {
    archive: ArchivePointer,
    subdir: Option<PathBuf>,
    server_handle: Option<Arc<tokio::task::JoinHandle<()>>>,
    rx: Option<Arc<RwLock<mpsc::Receiver<Bytes>>>>,
    port: u16,
}

impl ApiReceiver {
    pub fn new(port: u16) -> Self {
        let (tx, rx) = mpsc::channel::<Bytes>(1);
        let rx = Arc::new(RwLock::new(rx));
        let rx_clone = rx.clone();

        // Start the Axum server
        let handle = tokio::spawn(async move {
            let app = Router::new()
                .route("/", get(Self::index_handler))
                .route(
                    "/upload",
                    post(move |multipart: AxumMultipart| {
                        let tx = tx.clone();
                        async move { Self::upload_handler(multipart, tx).await }
                    }),
                )
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
                    </style>
                </head>
                <body>
                    <h1>Elasticsearch Diagnostics Upload</h1>
                    <div class="upload-form">
                        <form action="/upload" method="post" enctype="multipart/form-data">
                            <p>Select a diagnostic bundle (ZIP file):</p>
                            <input type="file" name="file" accept=".zip" required>
                            <br>
                            <input type="submit" value="Upload" class="button">
                        </form>
                    </div>
                </body>
            </html>
            "#,
        )
    }

    async fn upload_handler(
        mut multipart: AxumMultipart,
        tx: mpsc::Sender<Bytes>,
    ) -> impl IntoResponse {
        // Process the multipart form
        while let Ok(Some(field)) = multipart.next_field().await {
            if field.name() == Some("file") {
                // Check if the file has a valid filename
                let file_name = match field.file_name() {
                    Some(file_name) if !file_name.ends_with(".zip") => {
                        return (
                            StatusCode::BAD_REQUEST,
                            "Invalid file type. Only .zip files are allowed.".to_string(),
                        );
                    }
                    Some(file_name) => file_name.to_string(),
                    None => {
                        return (StatusCode::BAD_REQUEST, "No file name provided".to_string());
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

                        // Send the bytes through the channel
                        if tx.send(bytes).await.is_ok() {
                            return (StatusCode::OK, message);
                        } else {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "Failed to process the upload".to_string(),
                            );
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to read upload data: {}", e);
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Failed to read upload data: {}", e),
                        );
                    }
                }
            }
        }

        (
            StatusCode::BAD_REQUEST,
            "No file part in the request".to_string(),
        )
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
