// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    Identifiers, ServerEvent, ServerState, Signals, job_feed_event, receiver_stream, signal_event,
    template, template_event,
};
use crate::{
    data::{KnownHost, KnownHostBuilder},
    processor::{Processor, new_job_id},
    receiver::Receiver,
};
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
    #[cfg(feature = "keystore")]
    {
        if let Err(err) =
            super::keystore::ensure_unlocked_for_active_output(&state, &request_user).await
        {
            send_event(
                &tx,
                job_feed_event(template::JobFailed {
                    job_id: new_job_id(),
                    error: &err,
                    source: &uri,
                }),
            )
            .await;
            return;
        }
    }

    let host = match KnownHostBuilder::new(signals.es_api.url.into())
        .apikey(Some(signals.es_api.key))
        .build()
    {
        Ok(host) => host,
        Err(e) => {
            state.record_failure().await;
            let error_msg = format!("Failed to build host: {}", e);
            log::error!("Failed to build host: {}", e);
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
    let source = &host.get_url().to_string();

    let receiver = match Receiver::try_from(host) {
        Ok(receiver) => {
            log::info!("Created receiver: {}", receiver);
            Arc::new(receiver)
        }
        Err(e) => {
            state.record_failure().await;
            let error_msg = format!("Failed to create receiver: {}", e);
            log::error!("Failed to create receiver: {}", e);
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

    let exporter = Arc::new(state.exporter.read().await.clone());
    let identifiers = Identifiers {
        user: Some(request_user),
        ..signals.metadata
    };

    let job = match Processor::try_new(receiver, exporter, identifiers).await {
        Ok(job) => job,
        Err(error) => {
            state.record_failure().await;
            send_event(
                &tx,
                template_event(template::JobFailed {
                    job_id: new_job_id(),
                    error: &error.to_string(),
                    source,
                }),
            )
            .await;
            return;
        }
    };

    match job.start().await {
        Ok(job) => {
            state.record_job_started().await;
            send_event(
                &tx,
                job_feed_event(template::JobProcessing {
                    job_id: job.id,
                    source,
                }),
            )
            .await;
            send_event(&tx, signal_event(r#"{"loading":false,"processing":true}"#)).await;

            match job.process().await {
                Ok(job) => {
                    let report = &job.state.report;
                    state
                        .record_success(report.diagnostic.docs.total, report.diagnostic.docs.errors)
                        .await;
                    send_event(
                        &tx,
                        template_event(template::JobCompleted {
                            job_id: job.id,
                            diagnostic_id: &report.diagnostic.metadata.id,
                            docs_created: &report.diagnostic.docs.created,
                            duration: &format!(
                                "{:.3}",
                                report.diagnostic.processing_duration as f64 / 1000.0
                            ),
                            source,
                            kibana_link: report
                                .diagnostic
                                .kibana_link
                                .as_ref()
                                .unwrap_or(&"#".to_string()),
                            product: &report.diagnostic.product.to_string(),
                        }),
                    )
                    .await;
                }
                Err(job) => {
                    state.record_failure().await;
                    send_event(
                        &tx,
                        template_event(template::JobFailed {
                            job_id: job.id,
                            error: &job.state.error,
                            source,
                        }),
                    )
                    .await;
                }
            };
            send_event(
                &tx,
                signal_event(format!(
                    r#"{{"es_api":{{"url":"","key":""}},"processing":false,"stats":{}}}"#,
                    state.get_stats().await
                )),
            )
            .await;
        }
        Err(job) => {
            state.record_failure().await;
            send_event(
                &tx,
                job_feed_event(template::JobFailed {
                    job_id: job.id,
                    error: &job.state.error,
                    source,
                }),
            )
            .await;
            send_event(
                &tx,
                signal_event(format!(
                    r#"{{"processing":false,"stats":{}}}"#,
                    state.get_stats().await
                )),
            )
            .await;
        }
    };

    send_event(
        &tx,
        signal_event(format!(
            r#"{{"processing":false,"stats":{}}}"#,
            state.get_stats().await
        )),
    )
    .await;
}

async fn run_api_key_id(
    state: Arc<ServerState>,
    job_id: u64,
    request_user: String,
    tx: mpsc::Sender<ServerEvent>,
) {
    #[cfg(feature = "keystore")]
    {
        if let Err(err) =
            super::keystore::ensure_unlocked_for_active_output(&state, &request_user).await
        {
            send_event(
                &tx,
                template_event(template::JobFailed {
                    job_id,
                    error: &err,
                    source: "output target",
                }),
            )
            .await;
            return;
        }
    }

    let (identifiers, host): (Identifiers, KnownHost) = match state.pop_key(job_id).await {
        Some((mut identifiers, host)) => {
            identifiers.user = Some(request_user);
            (identifiers, host)
        }
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

    let source = &host.get_url().to_string();
    send_event(
        &tx,
        template_event(template::JobProcessing { job_id, source }),
    )
    .await;

    let receiver = match Receiver::try_from(host) {
        Ok(receiver) => {
            log::info!("Created receiver: {}", receiver);
            Arc::new(receiver)
        }
        Err(e) => {
            state.record_failure().await;
            let error_msg = format!("Failed to create receiver: {}", e);
            log::error!("Failed to create receiver: {}", e);
            send_event(
                &tx,
                template_event(template::JobFailed {
                    job_id,
                    error: &error_msg,
                    source,
                }),
            )
            .await;
            return;
        }
    };

    let exporter = Arc::new(state.exporter.read().await.clone());
    let processor = match Processor::try_new(receiver, exporter, identifiers).await {
        Ok(job) => job,
        Err(error) => {
            state.record_failure().await;
            send_event(
                &tx,
                template_event(template::JobFailed {
                    job_id: new_job_id(),
                    error: &error.to_string(),
                    source,
                }),
            )
            .await;
            return;
        }
    };

    match processor.start().await {
        Ok(processor) => {
            state.record_job_started().await;
            send_event(
                &tx,
                job_feed_event(template::JobProcessing {
                    job_id: processor.id,
                    source,
                }),
            )
            .await;
            send_event(
                &tx,
                signal_event(format!(
                    r#"{{"loading":false,"processing":true,"es_api":{{"url":"{source}"}}}}"#
                )),
            )
            .await;

            match processor.process().await {
                Ok(completed) => {
                    let report = &completed.state.report;
                    state
                        .record_success(report.diagnostic.docs.total, report.diagnostic.docs.errors)
                        .await;
                    send_event(
                        &tx,
                        template_event(template::JobCompleted {
                            job_id: completed.id,
                            diagnostic_id: &report.diagnostic.metadata.id,
                            docs_created: &report.diagnostic.docs.created,
                            duration: &format!(
                                "{:.3}",
                                report.diagnostic.processing_duration as f64 / 1000.0
                            ),
                            source,
                            kibana_link: report
                                .diagnostic
                                .kibana_link
                                .as_ref()
                                .unwrap_or(&"#".to_string()),
                            product: &report.diagnostic.product.to_string(),
                        }),
                    )
                    .await;
                }
                Err(failed) => {
                    state.record_failure().await;
                    send_event(
                        &tx,
                        template_event(template::JobFailed {
                            job_id: failed.id,
                            error: &failed.state.error,
                            source,
                        }),
                    )
                    .await;
                }
            };
            send_event(
                &tx,
                signal_event(format!(
                    r#"{{"es_api":{{"url":"","key":""}},"loading":false,"processing":false,"stats":{}}}"#,
                    state.get_stats().await
                )),
            )
            .await;
        }
        Err(failed) => {
            state.record_failure().await;
            send_event(
                &tx,
                template_event(template::JobFailed {
                    job_id,
                    error: &failed.state.error,
                    source,
                }),
            )
            .await;
            send_event(
                &tx,
                signal_event(format!(
                    r#"{{"loading":false,"processing":false,"stats":{}}}"#,
                    state.get_stats().await
                )),
            )
            .await;
        }
    };

    send_event(
        &tx,
        signal_event(format!(
            r#"{{"processing":false,"stats":{}}}"#,
            state.get_stats().await
        )),
    )
    .await;
}
