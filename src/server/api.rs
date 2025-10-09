// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{ApiKeyRequest, ServerState, UploadServiceRequest};
use crate::{
    data::{KnownHostBuilder, Uri},
    processor::new_job_id,
};
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde_json::json;
use std::sync::Arc;
use url::Url;

pub async fn service_link(
    State(state): State<Arc<ServerState>>,
    Json(payload): Json<UploadServiceRequest>,
) -> impl IntoResponse {
    log::info!(
        "Received JSON elastic uploader request for: {}",
        payload.url
    );

    let job_id = new_job_id();

    // Construct the URL with token authentication
    let uploader_service_url = match Url::parse(&payload.url) {
        Ok(mut url) => {
            // Set username to "token" and password to the actual token
            if url.set_username("token").is_err() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": "Failed to set token in URL"
                    })),
                );
            }
            if &payload.token == "" {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": "Authorization token cannot be empty"
                    })),
                );
            }
            if url.set_password(Some(&payload.token)).is_err() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
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
                Json(json!({
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
                Json(json!({
                    "error": format!("Failed to create URI: {}", e)
                })),
            );
        }
    };

    if let Uri::Url(_) = uri {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "URL must be for the Elastic Upload Service"
            })),
        );
    }

    // Stash the username and (filename, URI) into the server state for later use
    state.push_link(job_id, payload.metadata, uri).await;

    // Respond with a JSON success
    (StatusCode::CREATED, Json(json!({"link_id": job_id})))
}

#[axum::debug_handler]
pub async fn api_key(
    State(state): State<Arc<ServerState>>,
    Json(payload): Json<ApiKeyRequest>,
) -> impl IntoResponse {
    log::info!("Received JSON api key request for: {}", payload.url);

    let job_id = new_job_id();

    // Build the known host from the URL
    let url = match Url::parse(&payload.url) {
        Ok(url) => url,
        Err(e) => {
            log::error!("Failed to parse URL: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Failed to parse URL: {}", e)
                })),
            );
        }
    };

    // Validate apikey is not empty or whitespace-only
    if payload.apikey.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "API key cannot be empty"
            })),
        );
    }

    let host = match KnownHostBuilder::new(url)
        .apikey(Some(payload.apikey))
        .build()
    {
        Ok(host) => host,
        Err(e) => {
            log::error!("Failed to build host: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Failed to build host: {}", e)
                })),
            );
        }
    };

    // Stash the username and (filename, URI) into the server state for later use
    state.push_key(job_id, payload.metadata, host).await;

    // Respond with a JSON success
    (StatusCode::CREATED, Json(json!({"key_id": job_id})))
}
