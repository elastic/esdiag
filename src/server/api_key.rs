use super::{
    Identifiers, ServerState, Signals, patch_job_feed, patch_signals, patch_template, template,
};
use crate::{
    client::KnownHostBuilder,
    processor::{JobNew, new_job_id},
    receiver::Receiver,
};
use async_stream::stream;
use axum::{
    extract::State,
    response::{IntoResponse, Sse},
};
use datastar::axum::ReadSignals;
use std::sync::Arc;

pub async fn handler(
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

        let receiver = match Receiver::try_from(host) {
            Ok(receiver) => receiver,
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

        let exporter = {
            state.exporter.read().await.clone().with_identifiers(Identifiers {
                user: signals.metadata.user,
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
                yield patch_signals(r#"{"uploading":false,"processing":true}"#);

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
                    },
                    Err(job) => {
                        state.record_failure().await;
                        yield patch_template(template::JobFailed {
                            job_id: job.id,
                            error: &job.error,
                            source: job.filename.as_deref().unwrap_or(""),
                        });
                    }
                };
                yield patch_signals(&format!(r#"{{"es_api":{{"url":"","key":""}},"processing":false,"stats":{}}}"#, state.get_stats().await));
            },
            Err(job) => {
                state.record_failure().await;
                yield patch_job_feed(template::JobFailed {
                    job_id: job.id,
                    error: &job.error,
                    source: job.filename.as_deref().unwrap_or(""),
                });
                yield patch_signals(&format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await));
            },
        };

        yield patch_signals(&format!(r#"{{"processing":false,"stats":{}}}"#, state.get_stats().await));
    })
}
