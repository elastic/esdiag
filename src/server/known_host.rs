// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    KnownHostFormSignals, ServerEvent, ServerState, job_feed_event, job_runner, receiver_stream, signal_event, template,
};
use crate::{data::KnownHost, processor::new_job_id};
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Sse},
};
use datastar::axum::ReadSignals;
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

#[cfg(feature = "keystore")]
use crate::data::with_scoped_keystore_password;

const DOWNLOAD_REJECTION_TTL: Duration = Duration::from_secs(300);

fn saved_host_secret_error_message() -> &'static str {
    #[cfg(feature = "keystore")]
    {
        "Unlock the keystore before collecting from saved hosts that use stored secrets."
    }
    #[cfg(not(feature = "keystore"))]
    {
        "Keystore support is unavailable in this build, so saved hosts that use stored secrets cannot be collected."
    }
}

pub async fn form(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    ReadSignals(signals): ReadSignals<KnownHostFormSignals>,
) -> impl IntoResponse {
    let source = signals.job.collect.known_host.clone();
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
    let Some(host) = KnownHost::get_known(&signals.job.collect.known_host) else {
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
                source: &signals.job.collect.known_host,
            }),
        )
        .await;
        send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
        return;
    };

    let job_id = new_job_id();
    #[cfg(feature = "keystore")]
    let keystore_password = state.keystore_password().await;
    #[cfg(not(feature = "keystore"))]
    let keystore_password: Option<String> = None;
    if host.requires_keystore_secret() && keystore_password.is_none() {
        let error_message = saved_host_secret_error_message();
        state
            .reject_retained_bundle(&download_token, &request_user, error_message, DOWNLOAD_REJECTION_TTL)
            .await;
        state.record_failure().await;
        send_event(
            &tx,
            job_feed_event(template::JobFailed {
                job_id,
                error: error_message,
                source: &signals.job.collect.known_host,
            }),
        )
        .await;
        send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
        return;
    }

    let source = match host.get_url() {
        Ok(url) => url.to_string(),
        Err(e) => {
            let error_message = format!("Failed to resolve host URL: {}", e);
            state
                .reject_retained_bundle(&download_token, &request_user, error_message.clone(), DOWNLOAD_REJECTION_TTL)
                .await;
            state.record_failure().await;
            send_event(
                &tx,
                job_feed_event(template::JobFailed {
                    job_id,
                    error: &error_message,
                    source: &signals.job.collect.known_host,
                }),
            )
            .await;
            send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
            return;
        }
    };
    let job = super::JobRequest {
        identifiers: signals.metadata.clone(),
        input: super::JobInput::FromRemoteHost {
            source,
            host,
            diagnostic_type: signals.job.collect.diagnostic_type.clone(),
        },
    };

    #[cfg(feature = "keystore")]
    {
        if let Some(password) = keystore_password {
            with_scoped_keystore_password(password, async move {
                job_runner::run_job(state, signals.into(), job_id, request_user, tx, job, false).await;
            })
            .await;
        } else {
            job_runner::run_job(state, signals.into(), job_id, request_user, tx, job, false).await;
        }
    }
    #[cfg(not(feature = "keystore"))]
    {
        job_runner::run_job(state, signals.into(), job_id, request_user, tx, job, false).await;
    }
}
