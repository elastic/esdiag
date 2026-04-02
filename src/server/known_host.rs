// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    KnownHostFormSignals, ServerEvent, ServerState, job_feed_event, receiver_stream, signal_event,
    template, workflow,
};
use crate::{
    data::{KnownHost, with_scoped_keystore_password},
    processor::new_job_id,
};
use axum::{
    extract::State,
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
    ReadSignals(signals): ReadSignals<KnownHostFormSignals>,
) -> impl IntoResponse {
    let source = signals.workflow.collect.known_host.clone();
    let (tx, rx) = mpsc::channel(64);
    match state.resolve_user_email(&headers) {
        Ok((_, request_user)) => {
            tokio::spawn(async move {
                run_known_host_form(state, signals, request_user, tx).await;
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
                        source: &source,
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

pub(super) async fn run_known_host_form(
    state: Arc<ServerState>,
    signals: KnownHostFormSignals,
    request_user: String,
    tx: mpsc::Sender<ServerEvent>,
) {
    let download_token = signals.archive.download_token.clone();
    let Some(host) = KnownHost::get_known(&signals.workflow.collect.known_host) else {
        state
            .reject_retained_bundle(
                &download_token,
                &request_user,
                "Known host not found",
                DOWNLOAD_REJECTION_TTL,
            )
            .await;
        state.record_failure().await;
        send_event(
            &tx,
            job_feed_event(template::JobFailed {
                job_id: new_job_id(),
                error: "Known host not found",
                source: &signals.workflow.collect.known_host,
            }),
        )
        .await;
        send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
        return;
    };

    let job_id = new_job_id();
    let keystore_password = state.keystore_password_for(&request_user).await;
    if host.requires_keystore_secret() && keystore_password.is_none() {
        state
            .reject_retained_bundle(
                &download_token,
                &request_user,
                "Unlock the keystore before collecting from saved hosts that use stored secrets.",
                DOWNLOAD_REJECTION_TTL,
            )
            .await;
        state.record_failure().await;
        send_event(
            &tx,
            job_feed_event(template::JobFailed {
                job_id,
                error: "Unlock the keystore before collecting from saved hosts that use stored secrets.",
                source: &signals.workflow.collect.known_host,
            }),
        )
        .await;
        send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
        return;
    }

    let job = super::WorkflowJob {
        identifiers: signals.metadata.clone(),
        input: super::WorkflowInput::FromRemoteHost {
            source: host.get_url().to_string(),
            host,
            diagnostic_type: signals.workflow.collect.diagnostic_type.clone(),
        },
    };

    if let Some(password) = keystore_password {
        with_scoped_keystore_password(password, async move {
            workflow::run_job(state, signals.into(), job_id, request_user, tx, job, false).await;
        })
        .await;
    } else {
        workflow::run_job(state, signals.into(), job_id, request_user, tx, job, false).await;
    }
}
