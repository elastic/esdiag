use super::{ServerState, Signals, get_user_email, patch_signals, patch_template, template};
use crate::{
    data::diagnostic::report::Identifiers,
    processor::{JobNew, new_job_id},
    receiver::Receiver,
};
use async_stream::stream;
use axum::{
    extract::Multipart,
    http::HeaderMap,
    response::{Html, IntoResponse, Sse},
};
use bytes::Bytes;
use datastar::axum::ReadSignals;
use reqwest::StatusCode;
use std::sync::Arc;

pub async fn submit_handler(
    headers: HeaderMap,
    mut multipart: Multipart,
    state: Arc<ServerState>,
) -> impl IntoResponse {
    // Extract authenticated user email from header
    let _user_email = get_user_email(&headers);
    let job_id = new_job_id();

    // Process the multipart form
    if let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            // Check if the file has a valid filename
            let filename = match field.file_name() {
                Some(filename) if !filename.ends_with(".zip") => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Html(format!(
                            r#"<div id="job-{job_id}" class="status-box history-item error">
                        🛑 Invalid file type, only .zip files are allowed.
                    </div>"#
                        )),
                    );
                }
                Some(filename) => filename.to_string(),
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Html(format!(
                            r#"<div id="job-{job_id}" class="status-box history-item error">
                            🛑 Missing file name
                        </div>"#
                        )),
                    );
                }
            };

            let upload_file_element = format!(
                r#"<div id="job-{job_id}"
                    class="status-box history-item processing"
                    data-on-load="$uploading=false; $file_upload.job_id={job_id}; @post('upload/process')"
                >
                    <div class="spinner"></div> Processing diagnostic
                        <p><b>Filename:</b> {filename}</p>
                    </div>
                </div>"#
            );

            match field.bytes().await {
                Ok(data) => {
                    state.push_upload(job_id, filename, data).await;
                }
                Err(e) => {
                    let error_msg = format!("Failed to read upload data: {}", e);
                    log::error!("{}", error_msg);
                    state.record_failure().await;
                    return (
                        StatusCode::BAD_REQUEST,
                        Html(format!(
                            r#"<div id="job-{job_id}" class="status-box history-item error">
                            🛑 Error {error_msg}
                        </div>"#
                        )),
                    );
                }
            };

            (StatusCode::OK, Html(upload_file_element))
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(format!(
                    r#"<div id="job-{job_id}" class="status-box history-item error">
                        🛑 Upload Failed
                    </div>"#
                )),
            )
        }
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!(
                r#"<div id="job-{job_id}" class="status-box history-item error">
                    🛑 Upload Failed
                </div>"#
            )),
        )
    }
}

pub async fn process_hanlder(
    headers: HeaderMap,
    ReadSignals(signals): ReadSignals<Signals>,
    state: Arc<ServerState>,
) -> impl IntoResponse {
    // Extract authenticated user email from header
    let user_email = get_user_email(&headers);
    log::debug!("Signals: {:?}", signals);

    // Use the signal job_id to override the job.id created in this function
    let job_id = signals.file_upload.job_id;

    Sse::new(stream! {
        yield patch_signals(r#"{"processing":true}"#);
        let (filename, data): (String, Bytes) = match state.pop_upload(job_id).await{
            Some((filename, data)) => (filename, data),
            None =>{
                yield patch_template(template::Error {
                    id: "error-upload",
                    error: "Failed to process upload",
                    message: "Upload job_id not found"
                });
                return
            }
        };

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
                yield patch_template(template::Error {
                    id: "error-receiver",
                    error: "Failed to create upload receiver",
                    message: &error
                });
                return
            }
        };

        let exporter = {
            state.exporter.read().await.clone().with_identifiers(identifiers)
        };

        match JobNew::new(&exporter.identifiers(), receiver).ready(exporter).await {
            Ok(job) => {
                let job = job.start();
                // The submit function already pushed a template to the feed
                yield patch_template(template::JobProcessing {
                    job_id: job_id,
                    filename: job.filename.as_deref().unwrap_or(""),
                });

                match job.process().await {
                    Ok(job) => {
                        yield patch_template(template::JobCompleted {
                            job_id: job_id,
                            diagnostic_id: &job.report.metadata.id,
                            docs_created: &job.report.docs.created,
                            filename: job.filename.as_deref().unwrap_or(""),
                            kibana_link: job.report.kibana_link.as_ref().unwrap_or(&"#".to_string()),
                            product: &job.report.product.to_string(),
                        });
                        state.record_success(job.report.docs.total, job.report.docs.errors).await;
                    },
                    Err(job) => {
                        yield patch_template(template::JobFailed {
                            job_id: job_id,
                            error: &job.error,
                            source: job.filename.as_deref().unwrap_or(""),
                        });
                        state.record_failure().await;
                        state.job.record_failure(job).await;
                    }
                };
            },
            Err(job) => {
                yield patch_template(template::JobFailed {
                    job_id: job_id,
                    error: &job.error,
                    source: job.filename.as_deref().unwrap_or(""),
                });
                state.record_failure().await;
                state.job.record_failure(job).await;
            },
        };

        let stats = state.get_stats().await;
        yield patch_signals(&format!(r#"{{"processing":false,"stats":{stats}}}"#));
    })
}
