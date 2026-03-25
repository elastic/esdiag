// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::ServerState;
use async_stream::try_stream;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures::{StreamExt, stream::BoxStream};
use std::{sync::Arc, time::Duration};
use tokio::{fs::File, io::AsyncReadExt};

const RETAINED_BUNDLE_POST_DOWNLOAD_TTL: Duration = Duration::from_secs(300);
const DOWNLOAD_STREAM_CHUNK_SIZE: usize = 64 * 1024;

pub async fn download_retained_bundle(
    State(state): State<Arc<ServerState>>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (_, request_user) = match state.resolve_user_email(&headers) {
        Ok(result) => result,
        Err(err) => return (StatusCode::UNAUTHORIZED, err.to_string()).into_response(),
    };

    let Some(bundle) = state.retained_bundle(&token).await else {
        return (StatusCode::NOT_FOUND, "Download not found").into_response();
    };

    if bundle.owner != request_user {
        return (
            StatusCode::FORBIDDEN,
            "Download does not belong to this user",
        )
            .into_response();
    }

    if let Some(error) = bundle.error {
        state.discard_retained_bundle(&token).await;
        return (StatusCode::CONFLICT, error).into_response();
    }

    if bundle.expires_at_epoch <= super::now_epoch_seconds() {
        state.discard_retained_bundle(&token).await;
        return (StatusCode::GONE, "Download has expired").into_response();
    }

    let Some(path) = bundle.path else {
        return (StatusCode::ACCEPTED, "Download not ready").into_response();
    };
    let file = match File::open(&path).await {
        Ok(file) => file,
        Err(err) => {
            state.discard_retained_bundle(&token).await;
            return (
                StatusCode::NOT_FOUND,
                format!("Download is no longer available: {err}"),
            )
                .into_response();
        }
    };

    let _ = state
        .touch_retained_bundle(&token, RETAINED_BUNDLE_POST_DOWNLOAD_TTL)
        .await;
    state.schedule_retained_bundle_cleanup(token, RETAINED_BUNDLE_POST_DOWNLOAD_TTL);

    let safe_filename = bundle
        .filename
        .unwrap_or_else(|| "diagnostic.zip".to_string())
        .replace('"', "_");
    let disposition = format!("attachment; filename=\"{safe_filename}\"");

    let stream: BoxStream<'static, Result<Bytes, std::io::Error>> = try_stream! {
        let mut file = file;
        let mut buffer = vec![0_u8; DOWNLOAD_STREAM_CHUNK_SIZE];
        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }
            yield Bytes::copy_from_slice(&buffer[..bytes_read]);
        }
    }
    .boxed();

    let mut response = Response::new(Body::from_stream(stream));
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition)
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::download_retained_bundle;
    use crate::server::{RetainedBundle, now_epoch_seconds, test_server_state};
    use axum::{
        body::to_bytes,
        extract::{Path, State},
        http::{HeaderMap, StatusCode, header::CONTENT_DISPOSITION},
        response::IntoResponse,
    };
    use std::time::Duration;

    #[tokio::test]
    async fn download_retained_bundle_returns_zip_attachment() {
        let state = test_server_state();
        let path = std::env::temp_dir().join("esdiag-retained-bundle-test.zip");
        tokio::fs::write(&path, b"zip-bytes")
            .await
            .expect("write retained bundle");

        let token = state
            .insert_retained_bundle(
                "Anonymous".to_string(),
                "diagnostic.zip".to_string(),
                path,
                Duration::from_secs(60),
            )
            .await;

        let response = download_retained_bundle(State(state), Path(token), HeaderMap::new())
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_DISPOSITION).unwrap(),
            "attachment; filename=\"diagnostic.zip\""
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        assert_eq!(&body[..], b"zip-bytes");
    }

    #[tokio::test]
    async fn download_retained_bundle_discards_expired_entries() {
        let state = test_server_state();
        let token = "expired-token".to_string();
        let cleanup_dir = std::env::temp_dir().join("esdiag-retained-expired-bundle-test");
        let _ = tokio::fs::remove_dir_all(&cleanup_dir).await;
        tokio::fs::create_dir_all(&cleanup_dir)
            .await
            .expect("create cleanup dir");
        let path = cleanup_dir.join("expired.zip");
        tokio::fs::write(&path, b"expired")
            .await
            .expect("write expired bundle");
        state.retained_bundles.write().await.insert(
            token.clone(),
            RetainedBundle {
                owner: "Anonymous".to_string(),
                accepted: true,
                error: None,
                filename: Some("expired.zip".to_string()),
                path: Some(path.clone()),
                cleanup_path: Some(cleanup_dir.clone()),
                expires_at_epoch: now_epoch_seconds() - 1,
            },
        );

        let response =
            download_retained_bundle(State(state.clone()), Path(token.clone()), HeaderMap::new())
                .await
                .into_response();

        assert_eq!(response.status(), StatusCode::GONE);
        assert!(state.retained_bundle(&token).await.is_none());
        assert!(!path.exists(), "expired bundle file should be removed");
        assert!(
            !cleanup_dir.exists(),
            "expired bundle cleanup directory should be removed"
        );
    }
}
