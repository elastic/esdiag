// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{ApiKeyRequest, ServerState, UploadServiceRequest};
use crate::{
    data::{KnownHostBuilder, Uri},
    processor::{Processor, new_job_id},
    receiver::Receiver,
};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use url::Url;

#[derive(Deserialize)]
pub struct ServiceLinkQueryParams {
    #[serde(default, deserialize_with = "deserialize_empty_as_true")]
    wait_for_completion: bool,
}

#[axum::debug_handler]
pub async fn service_link(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Query(params): Query<ServiceLinkQueryParams>,
    Json(payload): Json<UploadServiceRequest>,
) -> impl IntoResponse {
    tracing::info!(
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
            if payload.token.is_empty() {
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
            tracing::error!("Invalid URL provided: {}", e);
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
            tracing::error!("Failed to create URI: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Failed to create URI: {}", e)
                })),
            );
        }
    };

    if matches!(&uri, Uri::Url(_)) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "URL must be for the Elastic Upload Service"
            })),
        );
    }

    let request_user = match state.resolve_user_email(&headers) {
        Ok((_, user)) => user,
        Err(err) => {
            tracing::warn!("Rejecting service_link request due to auth policy: {err}");
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": err.to_string()
                })),
            );
        }
    };

    let mut metadata = payload.metadata;
    metadata.user = Some(request_user);

    if params.wait_for_completion {
        tracing::info!("Processing service link synchronously: {}", job_id);
        tracing::debug!("[fsm][api.service_link] queued -> processing(sync): job_id={job_id}");

        let receiver = match Receiver::try_from(uri) {
            Ok(receiver) => {
                tracing::debug!("[fsm][api.service_link] receiver created: job_id={job_id}");
                Arc::new(receiver)
            }
            Err(e) => {
                tracing::error!("Failed to create receiver: {}", e);
                state.record_failure().await;
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": format!("Failed to create receiver: {}", e)
                    })),
                );
            }
        };

        let exporter = Arc::new(state.exporter.read().await.clone());
        tracing::debug!("[fsm][api.service_link] ready->try_new: job_id={job_id}");

        let processor = match Processor::try_new(receiver, exporter, metadata).await {
            Ok(processor) => {
                tracing::debug!(
                    "[fsm][api.service_link] try_new ok: processor_id={}, job_id={job_id}",
                    processor.id
                );
                processor
            }
            Err(error) => {
                tracing::error!("Failed to create processor: {}", error);
                state.record_failure().await;
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": format!("Failed to create processor: {}", error)
                    })),
                );
            }
        };

        let processing = match processor.start().await {
            Ok(processing) => {
                tracing::debug!(
                    "[fsm][api.service_link] start ok -> processing: processor_id={}, job_id={job_id}",
                    processing.id
                );
                processing
            }
            Err(failed) => {
                tracing::error!("Failed to start processor: {}", failed.state.error);
                state.record_failure().await;
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": format!("Failed to start processor: {}", failed.state.error)
                    })),
                );
            }
        };

        match processing.process().await {
            Ok(completed) => {
                tracing::debug!(
                    "[fsm][api.service_link] process ok -> completed: processor_id={}, job_id={job_id}",
                    completed.id
                );
                let report = &completed.state.report;
                state
                    .record_success(report.diagnostic.docs.total, report.diagnostic.docs.errors)
                    .await;

                let response = json!({
                    "diagnostic_id": report.diagnostic.metadata.id,
                    "kibana_link": report.diagnostic.kibana_link.as_deref().unwrap_or(""),
                    "took": completed.state.runtime
                });

                tracing::info!(
                    "Service link job completed synchronously: {}",
                    report.diagnostic.metadata.id
                );
                (StatusCode::OK, Json(response))
            }
            Err(failed) => {
                tracing::error!("Processing failed: {}", failed.state.error);
                state.record_failure().await;
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": format!("Processing failed: {}", failed.state.error)
                    })),
                )
            }
        }
    } else {
        // Stash the user-scoped metadata and (filename, URI) into the server state for later use
        tracing::debug!("[fsm][api.service_link] queued(in state): job_id={job_id}");
        state.push_link(job_id, metadata, uri).await;

        // Respond with a JSON success
        (StatusCode::CREATED, Json(json!({"link_id": job_id})))
    }
}

#[derive(Deserialize)]
pub struct ApiKeyQueryParams {
    #[serde(default, deserialize_with = "deserialize_empty_as_true")]
    wait_for_completion: bool,
}

/// Custom deserializer that treats empty string or "true" as true, and "false" or absence as false
fn deserialize_empty_as_true<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt.as_deref() {
        None => Ok(false),
        Some("") | Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(other) => Err(serde::de::Error::custom(format!(
            "Invalid boolean value: '{}'. Expected 'true', 'false', or empty string",
            other
        ))),
    }
}

#[axum::debug_handler]
pub async fn api_key(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Query(params): Query<ApiKeyQueryParams>,
    Json(payload): Json<ApiKeyRequest>,
) -> impl IntoResponse {
    tracing::info!("Received JSON api key request for: {}", payload.url);

    let job_id = new_job_id();
    tracing::debug!(
        "[fsm][api.api_key] start: job_id={}, wait_for_completion={}",
        job_id,
        params.wait_for_completion
    );

    let request_user = match state.resolve_user_email(&headers) {
        Ok((_, user)) => user,
        Err(err) => {
            tracing::warn!("Rejecting api_key request due to auth policy: {err}");
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": err.to_string()
                })),
            );
        }
    };

    // Build the known host from the URL
    let url = match Url::parse(&payload.url) {
        Ok(url) => url,
        Err(e) => {
            tracing::error!("Failed to parse URL: {}", e);
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
            tracing::error!("Failed to build host: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Failed to build host: {}", e)
                })),
            );
        }
    };

    // If wait_for_completion is true, process the job synchronously
    if params.wait_for_completion {
        tracing::info!("Processing job: {}", job_id);
        tracing::debug!("[fsm][api.api_key] queued -> processing(sync): job_id={job_id}");

        // Create receiver from host
        let receiver = match Receiver::try_from(host) {
            Ok(receiver) => {
                tracing::info!("Created receiver: {}", receiver);
                tracing::debug!("[fsm][api.api_key] receiver created: job_id={job_id}");
                Arc::new(receiver)
            }
            Err(e) => {
                tracing::error!("Failed to create receiver: {}", e);
                state.record_failure().await;
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": format!("Failed to create receiver: {}", e)
                    })),
                );
            }
        };

        let exporter = Arc::new(state.exporter.read().await.clone());
        let mut identifiers = payload.metadata;
        identifiers.user = Some(request_user.clone());
        tracing::debug!("[fsm][api.api_key] ready->try_new: job_id={job_id}");

        // Create and start the processor
        let processor = match Processor::try_new(receiver, exporter, identifiers).await {
            Ok(processor) => {
                tracing::debug!(
                    "[fsm][api.api_key] try_new ok: processor_id={}, job_id={job_id}",
                    processor.id
                );
                processor
            }
            Err(error) => {
                tracing::error!("Failed to create processor: {}", error);
                state.record_failure().await;
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": format!("Failed to create processor: {}", error)
                    })),
                );
            }
        };

        tracing::debug!(
            "[fsm][api.api_key] ready->start: processor_id={}, job_id={job_id}",
            processor.id
        );
        let processing = match processor.start().await {
            Ok(processing) => {
                tracing::debug!(
                    "[fsm][api.api_key] start ok -> processing: processor_id={}, job_id={job_id}",
                    processing.id
                );
                processing
            }
            Err(failed) => {
                tracing::error!("Failed to start processor: {}", failed.state.error);
                state.record_failure().await;
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": format!("Failed to start processor: {}", failed.state.error)
                    })),
                );
            }
        };

        // Process the job
        tracing::debug!(
            "[fsm][api.api_key] processing->process await: processor_id={}, job_id={job_id}",
            processing.id
        );
        match processing.process().await {
            Ok(completed) => {
                tracing::debug!(
                    "[fsm][api.api_key] process ok -> completed: processor_id={}, job_id={job_id}",
                    completed.id
                );
                let report = &completed.state.report;
                state
                    .record_success(report.diagnostic.docs.total, report.diagnostic.docs.errors)
                    .await;

                let response = json!({
                    "diagnostic_id": report.diagnostic.metadata.id,
                    "kibana_link": report.diagnostic.kibana_link.as_deref().unwrap_or(""),
                    "took": completed.state.runtime
                });

                tracing::info!(
                    "Job completed successfully: {}",
                    report.diagnostic.metadata.id
                );
                (StatusCode::OK, Json(response))
            }
            Err(failed) => {
                tracing::debug!(
                    "[fsm][api.api_key] process failed -> failed: processor_id={}, job_id={job_id}",
                    failed.id
                );
                tracing::error!("Processing failed: {}", failed.state.error);
                state.record_failure().await;
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": format!("Processing failed: {}", failed.state.error)
                    })),
                )
            }
        }
    } else {
        // Stash the username and (filename, URI) into the server state for later use
        tracing::debug!("[fsm][api.api_key] queued(in state): job_id={job_id}");
        let mut metadata = payload.metadata;
        metadata.user = Some(request_user);
        state.push_key(job_id, metadata, host).await;

        // Respond with a JSON success
        (StatusCode::CREATED, Json(json!({"key_id": job_id})))
    }
}
