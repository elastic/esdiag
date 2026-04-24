// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use bytes::BytesMut;
use eyre::{Result, eyre};
use reqwest::{Client, ClientBuilder, StatusCode};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::{fs::File, io::AsyncReadExt};

const CHUNK_SIZE: usize = 50 * 1024 * 1024;
const UPLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const UPLOAD_REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
pub const DEFAULT_UPLOAD_API_URL: &str = "https://upload.elastic.co";

#[derive(Debug, Deserialize)]
pub struct FinalizeResponse {
    pub slug: String,
    pub token: String,
}

pub async fn upload_file(file_path: &Path, upload_id: &str, api_url: &str) -> Result<FinalizeResponse> {
    let client = build_http_client()?;
    let upload_id = normalize_upload_id(upload_id);
    let filename = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| eyre!("Invalid upload file name"))?
        .to_string();

    ensure_upload_exists(&client, &upload_id, api_url).await?;

    let file_digest = digest_file(file_path).await?;
    upload_parts(&client, file_path, &filename, &upload_id, &file_digest, api_url).await?;
    finalize_upload(&client, &upload_id, &file_digest, api_url).await
}

pub fn normalize_upload_id(upload_id: &str) -> String {
    upload_id
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(upload_id)
        .to_string()
}

fn build_http_client() -> Result<Client> {
    Ok(ClientBuilder::new()
        .connect_timeout(UPLOAD_CONNECT_TIMEOUT)
        .timeout(UPLOAD_REQUEST_TIMEOUT)
        .build()?)
}

async fn ensure_upload_exists(client: &Client, upload_id: &str, api_url: &str) -> Result<()> {
    let response = client.head(format!("{api_url}/api/uploads/{upload_id}")).send().await?;

    if response.status() == StatusCode::OK {
        Ok(())
    } else {
        Err(eyre!(
            "Upload id '{}' does not exist (status {})",
            upload_id,
            response.status()
        ))
    }
}

async fn digest_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];

    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hex_digest(hasher.finalize()))
}

async fn upload_parts(
    client: &Client,
    file_path: &Path,
    filename: &str,
    upload_id: &str,
    file_digest: &str,
    api_url: &str,
) -> Result<()> {
    let mut file = File::open(file_path).await?;
    let mut part_number: i64 = 1;
    let mut chunk = BytesMut::with_capacity(CHUNK_SIZE);

    loop {
        chunk.clear();
        let bytes_read = file.read_buf(&mut chunk).await?;
        if bytes_read == 0 {
            break;
        }

        let mut part_hasher = Sha256::new();
        part_hasher.update(&chunk);
        let part_digest = hex_digest(part_hasher.finalize());
        let body = chunk.split().freeze();

        let response = client
            .put(format!("{api_url}/api/uploads/{upload_id}"))
            .query(&[
                ("filename", filename),
                ("file_digest", file_digest),
                ("part_number", &part_number.to_string()),
                ("part_digest", &part_digest),
            ])
            .body(body)
            .send()
            .await?;

        if response.status() != StatusCode::CREATED && response.status() != StatusCode::CONFLICT {
            return Err(eyre!("Failed to upload part {}: {}", part_number, response.status()));
        }

        part_number += 1;
    }

    Ok(())
}

async fn finalize_upload(
    client: &Client,
    upload_id: &str,
    file_digest: &str,
    api_url: &str,
) -> Result<FinalizeResponse> {
    let response = client
        .post(format!("{api_url}/api/uploads/{upload_id}/{file_digest}/_finalize"))
        .send()
        .await?;

    if response.status() != StatusCode::OK {
        return Err(eyre!(
            "Failed to finalize upload '{}': {}",
            upload_id,
            response.status()
        ));
    }

    Ok(response.json::<FinalizeResponse>().await?)
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn default_upload_path(file_name: &str) -> PathBuf {
    PathBuf::from(file_name)
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_UPLOAD_API_URL, default_upload_path, normalize_upload_id, upload_file};
    use axum::{
        Router,
        extract::{Path as AxumPath, Query, State},
        http::StatusCode,
        routing::{head, post},
    };
    use serde::Deserialize;
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };
    use tokio::net::TcpListener;

    #[test]
    fn normalize_upload_id_uses_last_path_segment() {
        assert_eq!(normalize_upload_id("abc123"), "abc123");
        assert_eq!(normalize_upload_id("https://upload.elastic.co/g/abc123"), "abc123");
    }

    #[test]
    fn default_upload_path_preserves_cli_filename() {
        assert_eq!(default_upload_path("diag.zip").to_string_lossy(), "diag.zip");
        assert_eq!(DEFAULT_UPLOAD_API_URL, "https://upload.elastic.co");
    }

    #[derive(Clone, Default)]
    struct TestState {
        head_calls: Arc<AtomicUsize>,
        put_calls: Arc<AtomicUsize>,
        post_calls: Arc<AtomicUsize>,
    }

    #[derive(Debug, Deserialize)]
    struct UploadQuery {
        filename: String,
        file_digest: String,
        part_number: String,
        part_digest: String,
    }

    async fn head_upload(State(state): State<TestState>, AxumPath(_upload_id): AxumPath<String>) -> StatusCode {
        state.head_calls.fetch_add(1, Ordering::SeqCst);
        StatusCode::OK
    }

    async fn put_upload(
        State(state): State<TestState>,
        AxumPath(_upload_id): AxumPath<String>,
        Query(query): Query<UploadQuery>,
        body: axum::body::Bytes,
    ) -> StatusCode {
        state.put_calls.fetch_add(1, Ordering::SeqCst);
        assert!(query.filename.ends_with(".zip"));
        assert_eq!(query.part_number, "1");
        assert!(!query.file_digest.is_empty());
        assert!(!query.part_digest.is_empty());
        assert_eq!(body.as_ref(), b"diagnostic payload");
        StatusCode::CREATED
    }

    async fn finalize_upload_handler(
        State(state): State<TestState>,
        AxumPath((upload_id, file_digest)): AxumPath<(String, String)>,
    ) -> (StatusCode, axum::Json<HashMap<&'static str, String>>) {
        state.post_calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(upload_id, "abc123");
        assert!(!file_digest.is_empty());
        (
            StatusCode::OK,
            axum::Json(HashMap::from([
                ("slug", "abc123".to_string()),
                ("token", "secret-token".to_string()),
            ])),
        )
    }

    #[tokio::test]
    async fn upload_file_uses_expected_service_protocol() {
        let state = TestState::default();
        let app = Router::new()
            .route("/api/uploads/{upload_id}", head(head_upload).put(put_upload))
            .route(
                "/api/uploads/{upload_id}/{file_digest}/_finalize",
                post(finalize_upload_handler),
            )
            .with_state(state.clone());

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind test listener");
        let addr = listener.local_addr().expect("listener addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve uploader stub");
        });

        let mut file = tempfile::Builder::new()
            .prefix("diag-")
            .suffix(".zip")
            .tempfile()
            .expect("temp file");
        std::io::Write::write_all(&mut file, b"diagnostic payload").expect("write payload");

        let response = upload_file(
            file.path(),
            "https://upload.elastic.co/g/abc123",
            &format!("http://{}", addr),
        )
        .await
        .expect("upload succeeds");

        assert_eq!(response.slug, "abc123");
        assert_eq!(response.token, "secret-token");
        assert_eq!(state.head_calls.load(Ordering::SeqCst), 1);
        assert_eq!(state.put_calls.load(Ordering::SeqCst), 1);
        assert_eq!(state.post_calls.load(Ordering::SeqCst), 1);

        server.abort();
    }
}
