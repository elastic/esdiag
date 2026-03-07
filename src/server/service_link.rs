// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    Identifiers, ServerEvent, ServerState, Signals, job_feed_event, receiver_stream, signal_event,
    template, template_event,
};
use crate::{
    data::Uri,
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
    log::info!(
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

async fn run_service_link_form(
    state: Arc<ServerState>,
    signals: Signals,
    request_user: String,
    tx: mpsc::Sender<ServerEvent>,
) {
    #[cfg(feature = "keystore")]
    {
        if let Err(err) = super::keystore::ensure_unlocked_for_active_output(&state).await {
            send_event(
                &tx,
                template_event(template::JobFailed {
                    job_id: new_job_id(),
                    error: &err,
                    source: "output target",
                }),
            )
            .await;
            return;
        }
    }

    let service_link = &signals.service_link;
    send_event(&tx, signal_event(r#"{"loading":true}"#)).await;

    let tokenized_uri = if let Uri::ServiceLinkNoAuth(mut url) = service_link.url.clone() {
        if url.set_username("token").is_err() {
            send_event(
                &tx,
                template_event(template::Error {
                    id: "error-url",
                    error: "Upload Service",
                    message: "Failed to set username in URL",
                }),
            )
            .await;
        }
        if url.set_password(Some(&service_link.token)).is_err() {
            send_event(
                &tx,
                template_event(template::Error {
                    id: "error-url",
                    error: "Upload Service",
                    message: "Failed to set token in URL",
                }),
            )
            .await;
        }
        Uri::ServiceLink(url)
    } else {
        let error_msg = format!("Unsupported URL: {}", service_link.url);
        log::error!("Invalid URL provided: {}", service_link.url);
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

    log::debug!("Tokenized URI: {}", tokenized_uri);
    let source = &signals.service_link.filename;
    let receiver = match Receiver::try_from(tokenized_uri) {
        Ok(receiver) => Arc::new(receiver),
        Err(e) => {
            state.record_failure().await;
            let error = &format!("Failed to create receiver: {}", e);
            log::error!("Failed to create receiver: {}", e);
            send_event(
                &tx,
                job_feed_event(template::JobFailed {
                    job_id: new_job_id(),
                    error,
                    source,
                }),
            )
            .await;
            send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
            return;
        }
    };

    let exporter = Arc::new(state.exporter.read().await.clone());
    let identifiers = Identifiers {
        user: Some(request_user),
        ..signals.metadata
    };
    let processor = match Processor::try_new(receiver, exporter, identifiers).await {
        Ok(ready) => ready,
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
                signal_event(
                    r#"{"loading":false,"processing":true,"service_link":{"_curl":"","token":"","url":"","filename":""}}"#,
                ),
            )
            .await;

            match processor.process().await {
                Ok(processor) => {
                    let report = &processor.state.report;
                    state
                        .record_success(report.diagnostic.docs.total, report.diagnostic.docs.errors)
                        .await;
                    send_event(
                        &tx,
                        template_event(template::JobCompleted {
                            job_id: processor.id,
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
                    send_event(
                        &tx,
                        signal_event(format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await)),
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
                    send_event(
                        &tx,
                        signal_event(format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await)),
                    )
                    .await;
                }
            }
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
            send_event(
                &tx,
                signal_event(format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await)),
            )
            .await;
        }
    };
}

async fn run_service_link_id(
    state: Arc<ServerState>,
    job_id: u64,
    request_user: String,
    tx: mpsc::Sender<ServerEvent>,
) {
    #[cfg(feature = "keystore")]
    {
        if let Err(err) = super::keystore::ensure_unlocked_for_active_output(&state).await {
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

    let (identifiers, uri): (Identifiers, Uri) = match state.pop_link(job_id).await {
        Some((mut identifiers, uri)) => {
            identifiers.user = Some(request_user);
            (identifiers, uri)
        }
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

    let source = &identifiers.filename.clone().unwrap_or("None".to_string());
    let receiver = match Receiver::try_from(uri) {
        Ok(receiver) => Arc::new(receiver),
        Err(e) => {
            state.record_failure().await;
            let error = &format!("Failed to create receiver: {}", e);
            log::error!("Failed to create receiver: {}", e);
            send_event(
                &tx,
                template_event(template::JobFailed {
                    job_id,
                    error,
                    source,
                }),
            )
            .await;
            send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
            return;
        }
    };

    send_event(&tx, signal_event(r#"{"loading":false,"processing":true}"#)).await;
    let exporter = Arc::new(state.exporter.read().await.clone());
    let processor = match Processor::try_new(receiver, exporter, identifiers).await {
        Ok(ready) => ready,
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
            send_event(&tx, signal_event(r#"{"loading":false,"processing":true}"#)).await;
            match processor.process().await {
                Ok(completed) => {
                    let report = &completed.state.report;
                    state
                        .record_success(report.diagnostic.docs.total, report.diagnostic.docs.errors)
                        .await;
                    send_event(
                        &tx,
                        template_event(template::JobCompleted {
                            job_id,
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
                    send_event(
                        &tx,
                        signal_event(format!(
                            r#"{{"processing":false,"service_link":{{"_curl":"","token":"","url":"","filename":""}},"stats":{}}}"#,
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
                    send_event(&tx, signal_event(r#"{"processing":false}"#)).await;
                }
            }
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
                signal_event(format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await)),
            )
            .await;
        }
    };
}
