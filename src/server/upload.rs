use super::{ServerState, get_iap_email, template};
use crate::{data::diagnostic::report::Identifiers, processor::JobNew, receiver::Receiver};
use askama::Template;
use async_stream::stream;
use axum::{
    extract::Multipart,
    http::HeaderMap,
    response::{IntoResponse, Sse},
};
use datastar::{
    consts::ElementPatchMode,
    prelude::{PatchElements, PatchSignals},
};
use std::{convert::Infallible, sync::Arc};

pub async fn handler(
    headers: HeaderMap,
    mut multipart: Multipart,
    state: Arc<ServerState>,
) -> impl IntoResponse {
    // Extract authenticated user email from header
    let user_email = get_iap_email(&headers);

    Sse::new(stream! {
        let signal = r#"{"uploading":true}"#;
        let sse_event = PatchSignals::new(signal).write_as_axum_sse_event();
        yield Ok::<_, Infallible>(sse_event);

        // Process the multipart form
        while let Ok(Some(field)) = multipart.next_field().await {
            if field.name() == Some("file") {
                // Check if the file has a valid filename
                let filename = match field.file_name() {
                    Some(filename) if !filename.ends_with(".zip") => {
                        let element = template::Error::new(
                            "error-file-type",
                            "Invalid file type",
                            "Only <code>.zip</code> files are allowed."
                        );
                        let sse_event = PatchElements::new(element).write_as_axum_sse_event();
                        yield Ok::<_, Infallible>(sse_event);
                        filename.to_string()
                    }
                    Some(filename) => filename.to_string(),
                    None => {
                        let element = template::Error::new(
                            "error-file-name",
                            "Missing file name",
                            "No file name provided"
                        );
                        let sse_event = PatchElements::new(element).write_as_axum_sse_event();
                        yield Ok::<_, Infallible>(sse_event);
                        "".to_string()
                    }
                };
                // Get the file data
                match field.bytes().await {
                    Ok(data) => {
                        let message = format!("Received upload: {} ({} bytes)", filename, data.len());
                        log::info!("{}", message);

                        let signal = r#"{"uploading":false,"processing":true}"#;
                        let sse_event = PatchSignals::new(signal).write_as_axum_sse_event();
                        yield Ok::<_, Infallible>(sse_event);

                        let identifiers = Identifiers {
                            account: None,
                            case_number: None,
                            filename: Some(filename.clone()),
                            user: user_email.clone(),
                            opportunity: None,
                        };

                        let receiver = match Receiver::try_from(data) {
                            Ok(receiver) => receiver,
                            Err(e) => {
                                let error = format!("Failed to create receiver: {}", e);
                                log::error!("{}", error);
                                let element = template::Error::new(
                                    "error-receiver",
                                    "Failed to create upload receiver",
                                    &error
                                );
                                let sse_event = PatchElements::new(element).write_as_axum_sse_event();
                                yield Ok::<_, Infallible>(sse_event);
                                break;
                            }
                        };

                        let exporter = {
                            state.exporter.read().await.clone().with_identifiers(identifiers)
                        };

                        match JobNew::new(&exporter.identifiers(), receiver).ready(exporter).await {
                            Ok(job) => {
                                let job = job.start();
                                let element = template::JobProcessing {
                                    job_id: job.id,
                                    filename: &job.filename,
                                }.render().expect("Failed to render JobProcessing template");
                                let sse_event = PatchElements::new(element).selector("#job-feed")
                                    .mode(ElementPatchMode::After).write_as_axum_sse_event();
                                yield Ok::<_, Infallible>(sse_event);

                                let elements = match job.process().await {
                                    Ok(job) => {
                                        state.record_success(job.report.docs.total, job.report.docs.errors).await;
                                        template::JobCompleted {
                                            job_id: job.id,
                                            diagnostic_id: &job.report.metadata.id,
                                            docs_created: &job.report.docs.created,
                                            filename: &job.filename,
                                            kibana_link: job.report.kibana_link.as_ref().unwrap_or(&"#".to_string()),
                                            product: &job.report.product.to_string(),
                                        }.render().unwrap_or(template::Error::new("error", "Render error", "Failed to render template"))
                                    },
                                    Err(job) => {
                                        state.record_failure().await;
                                        let elements = template::JobFailed {
                                            job_id: job.id,
                                            error: &job.error,
                                            filename: &job.filename,
                                        }.render().expect("Failed to render JobFailed template");
                                        state.job.record_failure(job).await;
                                        elements
                                    }
                                };
                                let sse_event = PatchElements::new(elements).write_as_axum_sse_event();
                                yield Ok::<_, Infallible>(sse_event);
                            },
                            Err(job) => {
                                state.record_failure().await;
                                let element = template::JobFailed {
                                    job_id: job.id,
                                    error: &job.error,
                                    filename: &job.filename,
                                }.render().expect("Failed to render JobFailed template");
                                state.job.record_failure(job).await;
                                let sse_event = PatchElements::new(element).selector("#job-feed")
                                    .mode(ElementPatchMode::After).write_as_axum_sse_event();
                                yield Ok::<_, Infallible>(sse_event);
                            },
                        };
                    }
                    Err(e) => {
                        state.record_failure().await;
                        let error_msg = format!("Failed to read upload data: {}", e);
                        let element = template::Status::new("error", &error_msg);
                        let sse_event = PatchElements::new(element).write_as_axum_sse_event();
                        yield Ok::<_, Infallible>(sse_event);
                        log::error!("{}", error_msg);
                    }
                }
            }
        }
        let stats = state.get_stats().await;
        let signal = format!(r#"{{"processing":false,"stats":{stats}}}"#);
        let sse_event = PatchSignals::new(signal).write_as_axum_sse_event();
        yield Ok::<_, Infallible>(sse_event)
    })
}
