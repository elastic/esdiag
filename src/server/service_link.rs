use super::{ServerState, get_user_email, patch_job_feed, patch_signals, patch_template, template};
use crate::{
    data::{Uri, diagnostic::report::Identifiers},
    processor::{JobNew, new_job_id},
    receiver::Receiver,
};
use async_stream::stream;
use axum::{
    extract::Multipart,
    http::HeaderMap,
    response::{IntoResponse, Sse},
};
use std::sync::Arc;
use url::Url;

pub async fn handler(
    headers: HeaderMap,
    mut multipart: Multipart,
    state: Arc<ServerState>,
) -> impl IntoResponse {
    // Extract authenticated user email from header
    let username = get_user_email(&headers);

    let mut token = String::new();
    let mut url = String::new();
    let mut filename = String::new();

    // Process the multipart form
    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap_or("");
        match field_name {
            "token" => {
                token = field.text().await.unwrap_or_default();
            }
            "url" => {
                url = field.text().await.unwrap_or_default();
            }
            "filename" => {
                filename = field.text().await.unwrap_or_default();
            }
            _ => {} // Ignore other fields
        }
    }

    log::info!("Received Elastic upload service request for: {}", url);

    Sse::new(stream! {
        yield patch_signals(r#"{"uploading":true}"#);

        // Construct the URL with token authentication
        let uploader_service_url = match Url::parse(&url) {
            Ok(mut url) => {
                // Set username to "token" and password to the actual token
                if url.set_username("token").is_err() {
                    yield patch_template(template::Error {
                        id: "error-url",
                        error: "Upload Service",
                        message: "Failed to set username in URL",
                    });
                    yield patch_signals(r#"{"uploading":false}"#);
                }
                if url.set_password(Some(&token)).is_err() {
                    yield patch_template(template::Error {
                        id: "error-url",
                        error: "Upload Service",
                        message: "Failed to set token in URL",
                    });
                    yield patch_signals(r#"{"uploading":false}"#);
                }
                url
            }
            Err(e) => {
                let error_msg = format!("Invalid URL: {}", e);
                log::error!("Invalid URL provided: {}", e);
                yield patch_template(template::Error {
                    id: "error-url",
                    error: "Upload Service",
                    message: &error_msg
                });
                yield patch_signals(r#"{"uploading":false}"#);
                return
            }
        };

        // Create URI from the URL
        let uri = match Uri::try_from(uploader_service_url.to_string()) {
            Ok(uri) => uri,
            Err(e) => {
                let error_msg = format!("Failed to create URI: {}", e);
                log::error!("Failed to create URI: {}", e);
                yield patch_template(template::Error {
                    id: "error-url",
                    error: "Upload Service",
                    message: &error_msg
                });
                yield patch_signals(r#"{"uploading":false}"#);
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
                    source: &filename,
                });
                yield patch_signals(r#"{"uploading":false}"#);
                return
            }
        };

        let identifiers = Identifiers {
            account: None,
            case_number: None,
            filename: Some(filename.clone()),
            opportunity: None,
            user: username,
        };

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
                yield patch_signals(r#"{"uploading":false,"processing":true}"#);

                match job.process().await {
                    Ok(job) => {
                        yield patch_template(template::JobCompleted {
                            job_id: job.id,
                            diagnostic_id: &job.report.metadata.id,
                            docs_created: &job.report.docs.created,
                            filename: job.filename.as_deref().unwrap_or(""),
                            kibana_link: job.report.kibana_link.as_ref().unwrap_or(&"#".to_string()),
                            product: &job.report.product.to_string(),
                        });
                        yield patch_signals(
                            &format!(r#"{{"processing":false,"service_link":{{"_curl":"","token":"","url":"","filename":""}},"stats":{}}}"#, state.get_stats().await)
                        );
                        state.record_success(job.report.docs.total, job.report.docs.errors).await;
                    }
                    Err(job) => {
                        yield patch_template(template::JobFailed {
                            job_id: job.id,
                            error: &job.error,
                            source: job.filename.as_deref().unwrap_or(""),
                        });
                        yield patch_signals(r#"{"processing":false}"#);
                        state.record_failure().await;
                        state.job.record_failure(job).await;
                    }
                }
            },
            Err(job) => {
                yield patch_template(template::JobFailed {
                    job_id: job.id,
                    error: &job.error,
                    source: job.filename.as_deref().unwrap_or(""),
                });
                yield patch_signals(r#"{"processing":false}"#);
                state.record_failure().await;
                state.job.record_failure(job).await;
            },
        };

    })
}
