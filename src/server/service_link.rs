// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    ServerEvent, ServerState, ServiceLinkFormSignals, WorkflowRunSignals, job_feed_event,
    receiver_stream, signal_event, template, template_event, workflow,
};
use crate::{
    data::{Uri, with_scoped_keystore_password},
    processor::new_job_id,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Sse},
};
use datastar::axum::ReadSignals;
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

const DOWNLOAD_REJECTION_TTL: Duration = Duration::from_secs(300);

pub async fn form(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    ReadSignals(signals): ReadSignals<ServiceLinkFormSignals>,
) -> impl IntoResponse {
    tracing::info!(
        "Received Elastic upload service request for: {}",
        signals.service_link.url
    );

    let (tx, rx) = mpsc::channel(64);
    match state.resolve_user_email(&headers) {
        Ok((_, request_user)) => {
            tokio::spawn(async move {
                run_service_link_form(state, signals, request_user, tx).await;
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
                        source: &signals.service_link.filename,
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
                run_service_link_id(state, job_id, request_user, tx).await;
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
                        source: "Forwarded service link job",
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

pub(super) async fn run_service_link_form(
    state: Arc<ServerState>,
    signals: ServiceLinkFormSignals,
    request_user: String,
    tx: mpsc::Sender<ServerEvent>,
) {
    let download_token = signals.archive.download_token.clone();
    if let Err(err) = super::ensure_active_output_ready(&state, &request_user).await {
        state
            .reject_retained_bundle(
                &download_token,
                &request_user,
                err.clone(),
                DOWNLOAD_REJECTION_TTL,
            )
            .await;
        send_event(
            &tx,
            job_feed_event(template::JobFailed {
                job_id: new_job_id(),
                error: &err,
                source: "output target",
            }),
        )
        .await;
        state.record_failure().await;
        send_event(&tx, signal_event(r#"{"loading":false,"processing":false}"#)).await;
        return;
    }

    let service_link = &signals.service_link;
    send_event(&tx, signal_event(r#"{"loading":true}"#)).await;

    let tokenized_uri = if let Uri::ServiceLinkNoAuth(mut url) = service_link.url.clone() {
        if url.set_username("token").is_err() {
            state
                .reject_retained_bundle(
                    &download_token,
                    &request_user,
                    "Failed to set username in URL",
                    DOWNLOAD_REJECTION_TTL,
                )
                .await;
            send_event(
                &tx,
                template_event(template::Error {
                    id: "error-url",
                    error: "Upload Service",
                    message: "Failed to set username in URL",
                }),
            )
            .await;
            send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
            return;
        }
        if url.set_password(Some(&service_link.token)).is_err() {
            state
                .reject_retained_bundle(
                    &download_token,
                    &request_user,
                    "Failed to set token in URL",
                    DOWNLOAD_REJECTION_TTL,
                )
                .await;
            send_event(
                &tx,
                template_event(template::Error {
                    id: "error-url",
                    error: "Upload Service",
                    message: "Failed to set token in URL",
                }),
            )
            .await;
            send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
            return;
        }
        Uri::ServiceLink(url)
    } else {
        let error_msg = format!("Unsupported URL: {}", service_link.url);
        state
            .reject_retained_bundle(
                &download_token,
                &request_user,
                error_msg.clone(),
                DOWNLOAD_REJECTION_TTL,
            )
            .await;
        tracing::error!("Invalid URL provided: {}", service_link.url);
        send_event(
            &tx,
            template_event(template::Error {
                id: "error-url",
                error: "Upload Service",
                message: &error_msg,
            }),
        )
        .await;
        send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
        return;
    };

    tracing::debug!("Tokenized URI: {}", tokenized_uri);
    let job_id = new_job_id();
    let job = super::WorkflowJob {
        identifiers: signals.metadata.clone(),
        input: super::WorkflowInput::FromServiceLink {
            source: signals.service_link.filename.clone(),
            uri: tokenized_uri,
        },
    };
    let keystore_password = state.keystore_password_for(&request_user).await;
    if let Some(password) = keystore_password {
        with_scoped_keystore_password(password, async move {
            workflow::run_job(state, signals.into(), job_id, request_user, tx, job, false).await;
        })
        .await;
    } else {
        workflow::run_job(state, signals.into(), job_id, request_user, tx, job, false).await;
    }
}

async fn run_service_link_id(
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
                    error: &format!("Link id {} not found", job_id),
                    source: "Forwarded service link job",
                }),
            )
            .await;
            send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
            return;
        }
    };
    workflow::run_job(
        state,
        WorkflowRunSignals::default(),
        job_id,
        request_user,
        tx,
        job,
        true,
    )
    .await;
}
