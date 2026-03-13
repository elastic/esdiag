// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    Identifiers, ServerEvent, ServerState, Signals, job_feed_event, receiver_stream, signal_event,
    template, template_event, workflow,
};
use crate::{data::KnownHostBuilder, processor::new_job_id};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Sse},
};
use datastar::axum::ReadSignals;
use std::sync::Arc;
use tokio::sync::mpsc;

pub async fn form(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    ReadSignals(signals): ReadSignals<Signals>,
) -> impl IntoResponse {
    // Extract authenticated user email from header
    let uri = signals.es_api.url.to_string();
    let (tx, rx) = mpsc::channel(64);
    match state.resolve_user_email(&headers) {
        Ok((_, request_user)) => {
            tokio::spawn(async move {
                run_api_key_form(state, signals, uri, request_user, tx).await;
            });
        }
        Err(err) => {
            tokio::spawn(async move {
                state.record_failure().await;
                send_event(
                    &tx,
                    job_feed_event(template::JobFailed {
                        job_id: new_job_id(),
                        error: &format!("Unauthorized request: {}", err),
                        source: &uri,
                    }),
                )
                .await;
                send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
            });
        }
    }
    Sse::new(receiver_stream(rx))
}

pub async fn id(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path(job_id): Path<u64>,
) -> impl IntoResponse {
    let (tx, rx) = mpsc::channel(64);
    match state.resolve_user_email(&headers) {
        Ok((_, request_user)) => {
            tokio::spawn(async move {
                run_api_key_id(state, job_id, request_user, tx).await;
            });
        }
        Err(err) => {
            tokio::spawn(async move {
                state.record_failure().await;
                send_event(
                    &tx,
                    template_event(template::JobFailed {
                        job_id,
                        error: &format!("Unauthorized request: {}", err),
                        source: "API key processing",
                    }),
                )
                .await;
                send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
            });
        }
    }
    Sse::new(receiver_stream(rx))
}

async fn send_event(tx: &mpsc::Sender<ServerEvent>, event: ServerEvent) {
    let _ = tx.send(event).await;
}

async fn run_api_key_form(
    state: Arc<ServerState>,
    signals: Signals,
    uri: String,
    request_user: String,
    tx: mpsc::Sender<ServerEvent>,
) {
    let host = match KnownHostBuilder::new(signals.es_api.url.clone().into())
        .apikey(Some(signals.es_api.key.clone()))
        .build()
    {
        Ok(host) => host,
        Err(e) => {
            state.record_failure().await;
            let error_msg = format!("Failed to build host: {}", e);
            tracing::error!("Failed to build host: {}", e);
            send_event(
                &tx,
                job_feed_event(template::JobFailed {
                    job_id: new_job_id(),
                    error: &error_msg,
                    source: &uri,
                }),
            )
            .await;
            return;
        }
    };
    let job = super::WorkflowJob {
        identifiers: Identifiers::default(),
        artifact: super::CollectedArtifact::RemoteCollection {
            source: host.get_url().to_string(),
            host,
            diagnostic_type: signals.workflow.collect.diagnostic_type.clone(),
        },
    };
    workflow::run_job(state, signals, new_job_id(), request_user, tx, job).await;
}

async fn run_api_key_id(
    state: Arc<ServerState>,
    job_id: u64,
    request_user: String,
    tx: mpsc::Sender<ServerEvent>,
) {
    let job = match state.pop_workflow_job(job_id).await {
        Some(job) => job,
        None => {
            send_event(
                &tx,
                template_event(template::JobFailed {
                    job_id,
                    error: &format!("API key id {} not found", job_id),
                    source: "API key processing",
                }),
            )
            .await;
            send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
            return;
        }
    };
    workflow::run_job(state, Signals::default(), job_id, request_user, tx, job).await;
}
