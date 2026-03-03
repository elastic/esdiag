// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{Identifiers, ServerState, Signals, patch_signals, patch_template, template};
use crate::{
    data::Uri,
    processor::{Processor, new_job_id},
    receiver::Receiver,
};
use async_stream::stream;
use axum::{
    extract::{Multipart, State},
    response::{Html, IntoResponse, Sse},
};
use bytes::Bytes;
use datastar::axum::ReadSignals;
use reqwest::StatusCode;
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

struct TempFileCleanup {
    path: PathBuf,
}

impl Drop for TempFileCleanup {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_file(&self.path) {
            log::debug!(
                "Failed to remove temp upload file {}: {}",
                self.path.display(),
                err
            );
        }
    }
}

pub async fn submit(
    State(state): State<Arc<ServerState>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
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
                    data-init="$loading=false; $file_upload.job_id={job_id}; @post('upload/process', {{openWhenHidden: true}})"
                >
                    <div class="spinner"></div> Processing diagnostic
                        <p><b>Filename:</b> {filename}</p>
                    </div>
                </div>"#
            );

            match field.bytes().await {
                Ok(data) => {
                    state.push_upload(job_id, filename, data).await;

                    // Add a cleanup task to prevent memory leaks if /upload/process is never called
                    let state_clone = state.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
                        if state_clone.pop_upload(job_id).await.is_some() {
                            log::warn!(
                                "Upload job {} was never processed and was removed from state to free memory",
                                job_id
                            );
                        }
                    });
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

pub async fn process(
    State(state): State<Arc<ServerState>>,
    ReadSignals(signals): ReadSignals<Signals>,
) -> impl IntoResponse {
    // Use the signal job_id to override the job.id created in this function
    let job_id = signals.file_upload.job_id;

    Sse::new(stream! {
        yield patch_signals(r#"{"processing":true}"#);
        let (filename, data): (String, Bytes) = match state.pop_upload(job_id).await{
            Some((filename, data)) => (filename, data),
            None =>{
                yield patch_signals(r#"{"processing":false}"#);
                yield patch_template(template::JobFailed {
                    job_id: job_id,
                    error: "Failed to upload file",
                    source: "User upload",
                });
                return
            }
        };

        let temp_upload_path =
            std::env::temp_dir().join(format!("esdiag-upload-{job_id}-{}.zip", Uuid::new_v4()));
        if let Err(e) = std::fs::write(&temp_upload_path, &data) {
            let error = format!("Failed to write temp upload file: {}", e);
            log::error!("{}", error);
            yield patch_signals(r#"{"processing":false}"#);
            yield patch_template(template::JobFailed {
                job_id: job_id,
                error: "Failed to stage uploaded file",
                source: &filename,
            });
            return
        }
        drop(data);
        let _temp_upload_cleanup = TempFileCleanup {
            path: temp_upload_path.clone(),
        };

        let receiver = match Receiver::try_from(Uri::File(temp_upload_path)) {
            Ok(receiver) => Arc::new(receiver),
            Err(e) => {
                let error = format!("Failed to create receiver: {}", e);
                log::error!("{}", error);
                yield patch_signals(r#"{"processing":false}"#);
                yield patch_template(template::JobFailed {
                    job_id: job_id,
                    error: "Failed to create file receiver",
                    source: &filename,
                });
                return
            }
        };

        let exporter = state.exporter.clone();

        let identifiers = Identifiers {
            user: signals.metadata.user,
            filename: Some(filename.clone()),
            ..signals.metadata
        };

        let processor = match Processor::try_new(receiver, exporter, identifiers).await {
            Ok(ready) => ready,
            Err(error) => {
                state.record_failure().await;
                yield patch_signals(r#"{"processing":false}"#);
                yield patch_template(template::JobFailed {
                    job_id,
                    error: &error.to_string(),
                    source: &filename,
                });
                return
            }
        };

        match processor.start().await {
            Ok(processor) => {
                yield patch_signals(r#"{"loading":false,"processing":true}"#);

                match processor.process().await {
                    Ok(completed) => {
                        let report = &completed.state.report;
                        state.record_success(report.diagnostic.docs.total, report.diagnostic.docs.errors).await;
                        yield patch_template(template::JobCompleted {
                            job_id: job_id,
                            diagnostic_id: &report.diagnostic.metadata.id,
                            docs_created: &report.diagnostic.docs.created,
                            duration: &format!("{:.3}", report.diagnostic.processing_duration as f64 / 1000.0),
                            source: &filename,
                            kibana_link: report.diagnostic.kibana_link.as_ref().unwrap_or(&"#".to_string()),
                            product: &report.diagnostic.product.to_string(),
                        });
                    },
                    Err(failed) => {
                        state.record_failure().await;
                        yield patch_template(template::JobFailed {
                            job_id: job_id,
                            error: &failed.state.error,
                            source: &filename,
                        });
                    }
                };
            },
            Err(failed) => {
                state.record_failure().await;
                yield patch_template(template::JobFailed {
                    job_id: job_id,
                    error: &failed.state.error,
                    source: &filename,
                });
            },
        };

        yield patch_signals(&format!(r#"{{"processing":false,"file_upload":{{"job_id":0}},"stats":{}}}"#, state.get_stats().await));
    })
}
