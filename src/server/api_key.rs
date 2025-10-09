// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    Identifiers, ServerState, Signals, patch_job_feed, patch_signals, patch_template, template,
};
use crate::{
    data::{KnownHost, KnownHostBuilder},
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
    // Extract authenticated user email from header
    let uri = signals.es_api.url.to_string();

    Sse::new(stream! {
        // Create receiver from URI
        let host = match KnownHostBuilder::new(signals.es_api.url.into()).apikey(Some(signals.es_api.key)).build() {
                Ok(host) => host,
                Err(e) => {
                    state.record_failure().await;
                    let error_msg = format!("Failed to build host: {}", e);
                    log::error!("Failed to build host: {}", e);
                    yield patch_job_feed(template::JobFailed{
                        job_id: new_job_id(),
                        error: &error_msg,
                        source: &uri
                    });
                    return
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
                yield patch_job_feed(template::JobFailed {
                    job_id: new_job_id(),
                    error: &error_msg,
                    source: &uri
                });
                return
            }
        };

        let exporter = state.exporter.clone();
        let identifiers = Identifiers {
            user: signals.metadata.user,
            ..signals.metadata
        };

        let job = match Processor::try_new(receiver, exporter, identifiers).await {
            Ok(job) => job,
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

        match job.start().await {
            Ok(job) => {
                yield patch_job_feed(template::JobProcessing {
                    job_id: job.id,
                    source
                });
                yield patch_signals(r#"{"loading":false,"processing":true}"#);

                match job.process().await {
                    Ok(job) => {
                        let report = &job.state.report;
                        state.record_success(report.docs.total, report.docs.errors).await;
                        yield patch_template(template::JobCompleted {
                            job_id: job.id,
                            diagnostic_id: &report.metadata.id,
                            docs_created: &report.docs.created,
                            duration: &format!("{:.3}", report.processing_duration as f64 / 1000.0),
                            source,
                            kibana_link: report.kibana_link.as_ref().unwrap_or(&"#".to_string()),
                            product: &report.product.to_string(),
                        });
                    },
                    Err(job) => {
                        state.record_failure().await;
                        yield patch_template(template::JobFailed {
                            job_id: job.id,
                            error: &job.state.error,
                            source,
                        });
                    }
                };
                yield patch_signals(&format!(r#"{{"es_api":{{"url":"","key":""}},"processing":false,"stats":{}}}"#, state.get_stats().await));
            },
            Err(job) => {
                state.record_failure().await;
                yield patch_job_feed(template::JobFailed {
                    job_id: job.id,
                    error: &job.state.error,
                    source,
                });
                yield patch_signals(&format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await));
            },
        };

        yield patch_signals(&format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await));
    })
}

pub async fn id(
    State(state): State<Arc<ServerState>>,
    Path(job_id): Path<u64>,
    ReadSignals(signals): ReadSignals<Signals>,
) -> impl IntoResponse {
    Sse::new(stream! {

        let (identifiers, host): (Identifiers, KnownHost) = match state.pop_key(job_id).await{
            Some((mut identifiers, host)) => {
                identifiers.user = signals.metadata.user;
                (identifiers, host)
            },
            None => {
                yield patch_template(template::JobFailed {
                    job_id,
                    error: &format!("API key id {} not found", job_id),
                    source: "API key processing"
                });
                yield patch_signals(r#"{"loading":false}"#);
                return
            }
        };

        let source = &host.get_url().to_string();
        yield patch_template(template::JobProcessing {
            job_id,
            source
        });

        let receiver = match Receiver::try_from(host) {
            Ok(receiver) => {
                log::info!("Created receiver: {}", receiver);
                Arc::new(receiver)
            }
            Err(e) => {
                state.record_failure().await;
                let error_msg = format!("Failed to create receiver: {}", e);
                log::error!("Failed to create receiver: {}", e);
                yield patch_template(template::JobFailed {
                    job_id,
                    error: &error_msg,
                    source,
                });
                return
            }
        };

        let exporter = state.exporter.clone();

        let processor = match Processor::try_new(receiver, exporter, identifiers).await {
            Ok(job) => job,
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
                yield patch_signals(&format!(r#"{{"loading":false,"processing":true,"es_api":{{"url":"{source}"}}}}"#));

                match processor.process().await {
                    Ok(completed) => {
                        let report = &completed.state.report;
                        state.record_success(report.docs.total, report.docs.errors).await;
                        yield patch_template(template::JobCompleted {
                            job_id: completed.id,
                            diagnostic_id: &report.metadata.id,
                            docs_created: &report.docs.created,
                            duration: &format!("{:.3}", report.processing_duration as f64 / 1000.0),
                            source,
                            kibana_link: report.kibana_link.as_ref().unwrap_or(&"#".to_string()),
                            product: &report.product.to_string(),
                        });
                    },
                    Err(failed) => {
                        state.record_failure().await;
                        yield patch_template(template::JobFailed {
                            job_id: failed.id,
                            error: &failed.state.error,
                            source,
                        });
                    }
                };
                yield patch_signals(&format!(r#"{{"es_api":{{"url":"","key":""}},"loading":false,"processing":false,"stats":{}}}"#, state.get_stats().await));
            },
            Err(failed) => {
                state.record_failure().await;
                yield patch_template(template::JobFailed {
                    job_id,
                    error: &failed.state.error,
                    source,
                });
                yield patch_signals(&format!(r#"{{"loading":false,"processing":false,"stats":{}}}"#, state.get_stats().await));
            },
        };

        yield patch_signals(&format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await));
    })
}
