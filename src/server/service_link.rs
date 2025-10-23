// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    Identifiers, ServerState, Signals, patch_job_feed, patch_signals, patch_template, template,
};
use crate::{
    data::Uri,
    processor::{Processor, new_job_id},
    receiver::Receiver,
};
use async_stream::stream;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Sse},
};
use datastar::axum::ReadSignals;
use std::sync::Arc;

pub async fn form(
    State(state): State<Arc<ServerState>>,
    ReadSignals(signals): ReadSignals<Signals>,
) -> impl IntoResponse {
    log::info!(
        "Received Elastic upload service request for: {}",
        signals.service_link.url
    );

    Sse::new(stream! {
        let service_link = &signals.service_link;
        yield patch_signals(r#"{"loading":true}"#);

        let tokenized_uri = if let Uri::ServiceLinkNoAuth(mut url) = service_link.url.clone() {
            // Set username to "token" and password to the actual token
            if url.set_username("token").is_err() {
                yield patch_template(template::Error {
                    id: "error-url",
                    error: "Upload Service",
                    message: "Failed to set username in URL",
                });
            }
            if url.set_password(Some(&service_link.token)).is_err() {
                yield patch_template(template::Error {
                    id: "error-url",
                    error: "Upload Service",
                    message: "Failed to set token in URL",
                });
            }
            Uri::ServiceLink(url)
        } else {
            let error_msg = format!("Unsupported URL: {}", service_link.url);
            log::error!("Invalid URL provided: {}", service_link.url);
            yield patch_template(template::Error {
                id: "error-url",
                error: "Upload Service",
                message: &error_msg
            });
            yield patch_signals(r#"{"loading":false}"#);
            return
        };

        log::debug!("Tokenized URI: {}", tokenized_uri);
        let source = &signals.service_link.filename;

        // Create receiver from URI
        let receiver = match Receiver::try_from(tokenized_uri) {
            Ok(receiver) => Arc::new(receiver),
            Err(e) => {
                state.record_failure().await;
                let error = &format!("Failed to create receiver: {}", e);
                log::error!("Failed to create receiver: {}", e);
                yield patch_job_feed(template::JobFailed {
                    job_id: new_job_id(),
                    error,
                    source,
                });
                yield patch_signals(r#"{"loading":false}"#);
                return
            }
        };

        let exporter = state.exporter.clone();
        let identifiers = Identifiers {
            user: signals.metadata.user,
            ..signals.metadata
        };

        let processor = match Processor::try_new(receiver, exporter, identifiers).await {
            Ok(ready) => ready,
            Err(error) => {
                state.record_failure().await;
                yield patch_template(template::JobFailed {
                    job_id: new_job_id(),
                    error: &error.to_string(),
                    source,
                });
                return
            }
        };

        match processor.start().await {
            Ok(processor) => {
                yield patch_job_feed(template::JobProcessing {
                    job_id: processor.id,
                    source
                });
                yield patch_signals(
                    r#"{"loading":false,"processing":true,"service_link":{"_curl":"","token":"","url":"","filename":""}}"#
                );

                match processor.process().await {
                    Ok(processor) => {
                        let report = &processor.state.report;
                        state.record_success(report.diagnostic.docs.total, report.diagnostic.docs.errors).await;
                        yield patch_template(template::JobCompleted {
                            job_id: processor.id,
                            diagnostic_id: &report.diagnostic.metadata.id,
                            docs_created: &report.diagnostic.docs.created,
                            duration: &format!("{:.3}", report.diagnostic.processing_duration as f64 / 1000.0),
                            source,
                            kibana_link: report.diagnostic.kibana_link.as_ref().unwrap_or(&"#".to_string()),
                            product: &report.diagnostic.product.to_string(),
                        });
                        yield patch_signals(
                            &format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await)
                        );
                    }
                    Err(failed) => {
                        state.record_failure().await;
                        yield patch_template(template::JobFailed {
                            job_id: failed.id,
                            error: &failed.state.error,
                            source,
                        });
                        yield patch_signals(&format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await));
                    }
                }
            },
            Err(failed) => {
                state.record_failure().await;
                yield patch_template(template::JobFailed {
                    job_id: failed.id,
                    error: &failed.state.error,
                    source
                });
                yield patch_signals(&format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await));
            },
        };
    })
}

pub async fn id(
    State(state): State<Arc<ServerState>>,
    Path(job_id): Path<u64>,
    ReadSignals(signals): ReadSignals<Signals>,
) -> impl IntoResponse {
    Sse::new(stream! {
        let (identifiers, uri): (Identifiers, Uri) = match state.pop_link(job_id).await{
            Some((mut identifiers, uri)) => {
                identifiers.user = signals.metadata.user;
                (identifiers, uri)
            },
            None => {
                yield patch_template(template::JobFailed {
                    job_id,
                    error: &format!("Link id {} not found", job_id),
                    source: "Forwarded service link job"
                });
                yield patch_signals(r#"{"loading":false}"#);
                return
            }
        };

        let source = &identifiers.filename.clone().unwrap_or("None".to_string());

        // Create receiver from URI
        let receiver = match Receiver::try_from(uri) {
            Ok(receiver) => Arc::new(receiver),
            Err(e) => {
                state.record_failure().await;
                let error = &format!("Failed to create receiver: {}", e);
                log::error!("Failed to create receiver: {}", e);
                yield patch_template(template::JobFailed {
                    job_id,
                    error,
                    source,
                });
                yield patch_signals(r#"{"loading":false}"#);
                return
            }
        };

        yield patch_signals(r#"{"loading":false,"processing":true}"#);

        let exporter = state.exporter.clone();

        let processor = match Processor::try_new(receiver, exporter, identifiers).await {
            Ok(ready) => ready,
            Err(error) => {
                state.record_failure().await;
                yield patch_template(template::JobFailed {
                    job_id: new_job_id(),
                    error: &error.to_string(),
                    source,
                });
                return
            }
        };

        match processor.start().await {
            Ok(processor) => {
                yield patch_job_feed(template::JobProcessing {
                    job_id: processor.id,
                    source
                });
                yield patch_signals(r#"{"loading":false,"processing":true}"#);
                match processor.process().await {
                    Ok(completed) => {
                        let report = &completed.state.report;
                        state.record_success(report.diagnostic.docs.total, report.diagnostic.docs.errors).await;
                        yield patch_template(template::JobCompleted {
                            job_id,
                            diagnostic_id: &report.diagnostic.metadata.id,
                            docs_created: &report.diagnostic.docs.created,
                            duration: &format!("{:.3}", report.diagnostic.processing_duration as f64 / 1000.0),
                            source,
                            kibana_link: report.diagnostic.kibana_link.as_ref().unwrap_or(&"#".to_string()),
                            product: &report.diagnostic.product.to_string(),
                        });
                        yield patch_signals(
                            &format!(r#"{{"processing":false,"service_link":{{"_curl":"","token":"","url":"","filename":""}},"stats":{}}}"#, state.get_stats().await)
                        );
                    }
                    Err(failed) => {
                        state.record_failure().await;
                        yield patch_template(template::JobFailed {
                            job_id,
                            error: &failed.state.error,
                            source,
                        });
                        yield patch_signals(r#"{"processing":false}"#);
                    }
                }
            },
            Err(failed) => {
                state.record_failure().await;
                yield patch_template(template::JobFailed {
                    job_id,
                    error: &failed.state.error,
                    source,
                });
                yield patch_signals(&format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await));
            },
        };
    })
}
