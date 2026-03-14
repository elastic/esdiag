// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    Identifiers, ServerEvent, ServerState, Signals, receiver_stream, signal_event, template,
    template_event,
};
use crate::{
    data::Uri,
    processor::{Processor, new_job_id},
    receiver::Receiver,
};
use axum::{
    extract::{Multipart, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Sse},
};
use bytes::Bytes;
use datastar::axum::ReadSignals;
use reqwest::StatusCode;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::mpsc;
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
    let can_use_keystore =
        cfg!(feature = "keystore") && state.runtime_mode_policy.allows_local_artifacts();

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
                    data-init="$loading=false; $file_upload.job_id={job_id}; if ({can_use_keystore} && $keystore.locked && $output.secure) {{ @get('/keystore/modal/process'); }} else {{ @post('upload/process', {{openWhenHidden: true}}); }}"
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
    headers: HeaderMap,
    ReadSignals(signals): ReadSignals<Signals>,
) -> impl IntoResponse {
    // Use the signal job_id to override the job.id created in this function
    let job_id = signals.file_upload.job_id;

    let (tx, rx) = mpsc::channel(64);
    match state.resolve_user_email(&headers) {
        Ok((_, request_user)) => {
            tokio::spawn(async move {
                run_upload_job(state, signals, job_id, request_user, tx).await;
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
                        source: "User upload",
                    }),
                )
                .await;
                send_terminal_signal(&tx, &state).await;
            });
        }
    }

    Sse::new(receiver_stream(rx))
}

async fn send_event(tx: &mpsc::Sender<ServerEvent>, event: ServerEvent) {
    // Processing must continue even when client disconnects.
    let _ = tx.send(event).await;
}

async fn send_terminal_signal(tx: &mpsc::Sender<ServerEvent>, state: &ServerState) {
    send_event(
        tx,
        signal_event(format!(
            r#"{{"processing":false,"file_upload":{{"job_id":0}},"stats":{}}}"#,
            state.get_stats().await
        )),
    )
    .await;
}

async fn run_upload_job(
    state: Arc<ServerState>,
    signals: Signals,
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
            send_terminal_signal(&tx, &state).await;
            return;
        }
    }

    send_event(&tx, signal_event(r#"{"processing":true}"#)).await;
    let (filename, data): (String, Bytes) = match state.pop_upload(job_id).await {
        Some((filename, data)) => (filename, data),
        None => {
            send_event(
                &tx,
                template_event(template::JobFailed {
                    job_id,
                    error: "Failed to upload file",
                    source: "User upload",
                }),
            )
            .await;
            send_terminal_signal(&tx, &state).await;
            return;
        }
    };

    let temp_upload_path =
        std::env::temp_dir().join(format!("esdiag-upload-{job_id}-{}.zip", Uuid::new_v4()));
    if let Err(e) = std::fs::write(&temp_upload_path, &data) {
        let error = format!("Failed to write temp upload file: {}", e);
        log::error!("{}", error);
        send_event(
            &tx,
            template_event(template::JobFailed {
                job_id,
                error: "Failed to stage uploaded file",
                source: &filename,
            }),
        )
        .await;
        send_terminal_signal(&tx, &state).await;
        return;
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
            send_event(
                &tx,
                template_event(template::JobFailed {
                    job_id,
                    error: "Failed to create file receiver",
                    source: &filename,
                }),
            )
            .await;
            send_terminal_signal(&tx, &state).await;
            return;
        }
    };

    let exporter = Arc::new(state.exporter.read().await.clone());
    let identifiers = Identifiers {
        user: Some(request_user),
        filename: Some(filename.clone()),
        ..signals.metadata
    };

    let processor = match Processor::try_new(receiver, exporter, identifiers).await {
        Ok(ready) => ready,
        Err(error) => {
            state.record_failure().await;
            send_event(
                &tx,
                template_event(template::JobFailed {
                    job_id,
                    error: &error.to_string(),
                    source: &filename,
                }),
            )
            .await;
            send_terminal_signal(&tx, &state).await;
            return;
        }
    };

    match processor.start().await {
        Ok(processor) => {
            state.record_job_started().await;
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
                            source: &filename,
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
                            job_id,
                            error: &failed.state.error,
                            source: &filename,
                        }),
                    )
                    .await;
                }
            };
        }
        Err(failed) => {
            state.record_failure().await;
            send_event(
                &tx,
                template_event(template::JobFailed {
                    job_id,
                    error: &failed.state.error,
                    source: &filename,
                }),
            )
            .await;
        }
    };

    send_terminal_signal(&tx, &state).await;
}

#[cfg(test)]
mod tests {
    use super::{run_upload_job, send_terminal_signal};
    use crate::server::{ServerEvent, Signals, test_server_state};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn run_upload_job_missing_upload_emits_failure_and_terminal_signal() {
        let state = test_server_state();
        let signals = Signals::default();
        let (tx, mut rx) = mpsc::channel(8);

        run_upload_job(state, signals, 42, "Anonymous".to_string(), tx).await;

        let mut saw_failure = false;
        let mut saw_terminal = false;
        while let Some(event) = rx.recv().await {
            match event {
                ServerEvent::Template(html) if html.contains("Failed to upload file") => {
                    saw_failure = true;
                }
                ServerEvent::Signals(payload) if payload.contains(r#""processing":false"#) => {
                    saw_terminal = true;
                }
                _ => {}
            }
        }

        assert!(saw_failure, "expected job failure template event");
        assert!(saw_terminal, "expected terminal processing=false signal");
    }

    #[tokio::test]
    async fn terminal_signal_includes_file_upload_reset() {
        let state = test_server_state();
        let (tx, mut rx) = mpsc::channel(4);

        send_terminal_signal(&tx, &state).await;
        drop(tx);

        let event = rx.recv().await.expect("expected one terminal signal");
        match event {
            ServerEvent::Signals(payload) => {
                assert!(payload.contains(r#""file_upload":{"job_id":0}"#));
            }
            _ => panic!("expected terminal signal payload"),
        }
    }

    #[tokio::test]
    async fn run_upload_job_completes_when_client_disconnected() {
        let state = test_server_state();
        let signals = Signals::default();
        let (tx, rx) = mpsc::channel(1);
        drop(rx);

        run_upload_job(state, signals, 999, "Anonymous".to_string(), tx).await;
    }
}
