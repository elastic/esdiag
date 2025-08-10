use super::{
    Identifiers, ServerState, Signals, patch_job_feed, patch_signals, patch_template, template,
};
use crate::{
    data::Uri,
    processor::{JobNew, new_job_id},
    receiver::Receiver,
};
use async_stream::stream;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Sse},
};
use datastar::axum::ReadSignals;
use std::sync::Arc;

pub async fn handler(
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

        // Create receiver from URI
        let receiver = match Receiver::try_from(tokenized_uri) {
            Ok(receiver) => receiver,
            Err(e) => {
                state.record_failure().await;
                let error_msg = format!("Failed to create receiver: {}", e);
                log::error!("Failed to create receiver: {}", e);
                yield patch_job_feed(template::JobFailed {
                    job_id: new_job_id(),
                    error: &error_msg,
                    source: &signals.service_link.filename,
                });
                yield patch_signals(r#"{"loading":false}"#);
                return
            }
        };

        let exporter = {
            state.exporter.read().await.clone().with_identifiers(Identifiers {
                filename: Some(signals.service_link.filename),
                ..signals.metadata
            })
        };

        match JobNew::new(&exporter.identifiers(), receiver).ready(exporter).await {
            Ok(job) => {
                let job = job.start();
                yield patch_job_feed(template::JobProcessing {
                    job_id: job.id,
                    filename: job.filename.as_deref().unwrap_or(""),
                });
                yield patch_signals(
                    r#"{"loading":false,"processing":true,"service_link":{"_curl":"","token":"","url":"","filename":""}}"#
                );

                match job.process().await {
                    Ok(job) => {
                        state.record_success(job.report.docs.total, job.report.docs.errors).await;
                        yield patch_template(template::JobCompleted {
                            job_id: job.id,
                            diagnostic_id: &job.report.metadata.id,
                            docs_created: &job.report.docs.created,
                            duration: &format!("{:.3}", job.report.processing_duration as f64 / 1000.0),
                            filename: job.filename.as_deref().unwrap_or(""),
                            kibana_link: job.report.kibana_link.as_ref().unwrap_or(&"#".to_string()),
                            product: &job.report.product.to_string(),
                        });
                        yield patch_signals(
                            &format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await)
                        );
                    }
                    Err(job) => {
                        state.record_failure().await;
                        yield patch_template(template::JobFailed {
                            job_id: job.id,
                            error: &job.error,
                            source: job.filename.as_deref().unwrap_or(""),
                        });
                        yield patch_signals(&format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await));
                    }
                }
            },
            Err(job) => {
                state.record_failure().await;
                yield patch_template(template::JobFailed {
                    job_id: job.id,
                    error: &job.error,
                    source: job.filename.as_deref().unwrap_or(""),
                });
                yield patch_signals(&format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await));
            },
        };
    })
}

pub async fn job_handler(
    State(state): State<Arc<ServerState>>,
    Path(id): Path<u64>,
    ReadSignals(signals): ReadSignals<Signals>,
) -> impl IntoResponse {
    Sse::new(stream! {
        let (identifiers, uri): (Identifiers, Uri) = match state.pop_link(id).await{
            Some((mut identifiers, uri)) => {
                identifiers.user = signals.metadata.user;
                (identifiers, uri)
            },
            None => {
                yield patch_job_feed(template::JobFailed {
                    job_id: id,
                    error: &format!("Link id {} not found", id),
                    source: "Forwarded service link job"
                });
                yield patch_signals(r#"{"loading":false}"#);
                return
            }
        };

        // Create receiver from URI
        let receiver = match Receiver::try_from(uri) {
            Ok(receiver) => receiver,
            Err(e) => {
                state.record_failure().await;
                let error_msg = format!("Failed to create receiver: {}", e);
                log::error!("Failed to create receiver: {}", e);
                yield patch_job_feed(template::JobFailed {
                    job_id: new_job_id(),
                    error: &error_msg,
                    source: &identifiers.filename.unwrap_or("None".to_string())
                });
                yield patch_signals(r#"{"loading":false}"#);
                return
            }
        };

        yield patch_signals(r#"{"loading":false,"processing":true}"#);

        let exporter = {
            state.exporter.read().await.clone().with_identifiers(identifiers)
        };

        match JobNew::new(&exporter.identifiers(), receiver).ready(exporter).await {
            Ok(job) => {
                let job = job.start();
                yield patch_job_feed(template::JobProcessing {
                    job_id: job.id,
                    filename: job.filename.as_deref().unwrap_or(""),
                });
                yield patch_signals(r#"{"loading":false,"processing":true}"#);

                match job.process().await {
                    Ok(job) => {
                        state.record_success(job.report.docs.total, job.report.docs.errors).await;
                        yield patch_template(template::JobCompleted {
                            job_id: job.id,
                            diagnostic_id: &job.report.metadata.id,
                            docs_created: &job.report.docs.created,
                            duration: &format!("{:.3}", job.report.processing_duration as f64 / 1000.0),
                            filename: job.filename.as_deref().unwrap_or(""),
                            kibana_link: job.report.kibana_link.as_ref().unwrap_or(&"#".to_string()),
                            product: &job.report.product.to_string(),
                        });
                        yield patch_signals(
                            &format!(r#"{{"processing":false,"service_link":{{"_curl":"","token":"","url":"","filename":""}},"stats":{}}}"#, state.get_stats().await)
                        );
                    }
                    Err(job) => {
                        state.record_failure().await;
                        yield patch_template(template::JobFailed {
                            job_id: job.id,
                            error: &job.error,
                            source: job.filename.as_deref().unwrap_or(""),
                        });
                        yield patch_signals(r#"{"processing":false}"#);
                    }
                }
            },
            Err(job) => {
                state.record_failure().await;
                yield patch_template(template::JobFailed {
                    job_id: job.id,
                    error: &job.error,
                    source: job.filename.as_deref().unwrap_or(""),
                });
                yield patch_signals(&format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await));
            },
        };
    })
}
