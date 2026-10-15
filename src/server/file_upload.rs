// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    ServerEvent, ServerState, UploadProcessSignals, job_runner, receiver_stream, replace_job_event, signal_event,
    template,
};
use crate::processor::new_job_id;
use axum::{
    extract::{Multipart, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Sse},
};
use datastar::axum::ReadSignals;
use reqwest::StatusCode;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::{fs::File, io::AsyncWriteExt};
use uuid::Uuid;

fn parse_scrubbed_checkbox_value(value: &str) -> bool {
    let value = value.trim();
    value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("on")
}

pub async fn submit(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let job_id = new_job_id();
    let can_use_keystore = cfg!(feature = "keystore") && state.server_policy.allows_local_runtime_features();
    let identity = match state.resolve_identity(&headers) {
        Ok(identity) => identity,
        Err(err) => {
            tracing::warn!("Diagnostic submission denied: {}", err);
            state.record_job_rejected().await;
            let (status, label) = super::identity_failure(&err);
            return (
                status,
                Html(format!(
                    r#"<div id="job-{job_id}" class="status-box history-item status-error">
                        🛑 {label}
                    </div>"#
                )),
            );
        }
    };
    let mut scrubbed_override: Option<bool> = None;
    let mut staged_upload: Option<(String, std::path::PathBuf)> = None;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(err) => return reject_malformed_upload(&state, job_id, staged_upload, err).await,
        };
        match field.name() {
            Some("file") => {
                if let Some((_filename, temp_upload_path)) = staged_upload.take() {
                    if let Err(remove_err) = tokio::fs::remove_file(&temp_upload_path).await
                        && remove_err.kind() != std::io::ErrorKind::NotFound
                    {
                        tracing::debug!(
                            "Failed to remove duplicate upload {}: {}",
                            temp_upload_path.display(),
                            remove_err
                        );
                    }
                    state.record_job_rejected().await;
                    return (
                        StatusCode::BAD_REQUEST,
                        Html(format!(
                            r#"<div id="job-{job_id}" class="status-box history-item status-error">
                                🛑 Multiple upload files are not supported
                            </div>"#
                        )),
                    );
                }

                let filename = match field.file_name() {
                    Some(filename) if !filename.ends_with(".zip") => {
                        state.record_job_rejected().await;
                        return (
                            StatusCode::BAD_REQUEST,
                            Html(format!(
                                r#"<div id="job-{job_id}" class="status-box history-item status-error">
                            🛑 Invalid file type, only .zip files are allowed.
                        </div>"#
                            )),
                        );
                    }
                    Some(filename) => filename.to_string(),
                    None => {
                        state.record_job_rejected().await;
                        return (
                            StatusCode::BAD_REQUEST,
                            Html(format!(
                                r#"<div id="job-{job_id}" class="status-box history-item status-error">
                                🛑 Missing file name
                            </div>"#
                            )),
                        );
                    }
                };

                let temp_upload_path =
                    std::env::temp_dir().join(format!("esdiag-upload-{job_id}-{}.zip", Uuid::new_v4()));
                match stage_upload_field(field, &temp_upload_path).await {
                    Ok(()) => staged_upload = Some((filename, temp_upload_path)),
                    Err(e) => {
                        if let Err(remove_err) = tokio::fs::remove_file(&temp_upload_path).await
                            && remove_err.kind() != std::io::ErrorKind::NotFound
                        {
                            tracing::debug!(
                                "Failed to remove partial upload {}: {}",
                                temp_upload_path.display(),
                                remove_err
                            );
                        }
                        let error_msg = format!("Failed to stage upload data: {}", e);
                        tracing::error!("{}", error_msg);
                        state.record_job_rejected().await;
                        return (
                            StatusCode::BAD_REQUEST,
                            Html(format!(
                                r#"<div id="job-{job_id}" class="status-box history-item status-error">
                                🛑 Error {error_msg}
                            </div>"#
                            )),
                        );
                    }
                }
            }
            Some("scrubbed") => {
                let mut field = field;
                let mut value = [0_u8; 64];
                let mut length = 0;
                loop {
                    let chunk = match field.chunk().await {
                        Ok(Some(chunk)) => chunk,
                        Ok(None) => break,
                        Err(err) => return reject_malformed_upload(&state, job_id, staged_upload, err).await,
                    };
                    if chunk.len() > value.len() - length {
                        return reject_malformed_upload(
                            &state,
                            job_id,
                            staged_upload,
                            "scrubbed field exceeds 64 bytes",
                        )
                        .await;
                    }
                    value[length..length + chunk.len()].copy_from_slice(&chunk);
                    length += chunk.len();
                }
                scrubbed_override = Some(
                    std::str::from_utf8(&value[..length])
                        .map(parse_scrubbed_checkbox_value)
                        .unwrap_or(false),
                );
            }
            _ => {}
        }
    }

    if let Some((filename, temp_upload_path)) = staged_upload {
        // The first processing patch replaces this card and removes data-init.
        // Keep its SSE request alive until the terminal card and signals arrive.
        let upload_file_element = format!(
            r#"<div id="job-{job_id}"
                class="status-box history-item status-processing"
                data-init="$loading=false; $file_upload.job_id={job_id}; if ({can_use_keystore} && $keystore.locked && $output.secure) {{ $_pending_job_action = 'upload-process'; $message = 'Unlock keystore to continue...'; @get('/keystore/modal/process', {{filterSignals: {{exclude: /.*/}}}}); }} else {{ @post('/upload/process', {{openWhenHidden: true, requestCancellation: 'disabled', filterSignals: {{include: /^(metadata|archive|job|file_upload)(\.|$)/}}}}); }}"
            >
                <div class="spinner"></div>
                <span>Processing diagnostic</span>
                <p><b>Filename:</b> {filename}</p>
            </div>"#,
            filename = askama::filters::escape(&filename, askama::filters::Html).expect("HTML escaping is infallible"),
        );

        state
            .push_upload(
                job_id,
                identity.user,
                identity.account,
                filename,
                temp_upload_path,
                scrubbed_override,
            )
            .await;

        let state_clone = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
            if let Some(job) = state_clone.pop_job_request(job_id).await {
                job.cleanup().await;
                tracing::warn!(
                    "Upload job {} was never processed and was removed from state to clean up the staged upload",
                    job_id
                );
            }
        });

        (StatusCode::OK, Html(upload_file_element))
    } else {
        state.record_job_rejected().await;
        (
            StatusCode::BAD_REQUEST,
            Html(format!(
                r#"<div id="job-{job_id}" class="status-box history-item status-error">
                    🛑 Missing upload file
                </div>"#
            )),
        )
    }
}

async fn reject_malformed_upload(
    state: &ServerState,
    job_id: u64,
    staged_upload: Option<(String, std::path::PathBuf)>,
    err: impl std::fmt::Display,
) -> (StatusCode, Html<String>) {
    tracing::warn!("Malformed upload request: {}", err);
    if let Some((_filename, temp_upload_path)) = staged_upload
        && let Err(remove_err) = tokio::fs::remove_file(&temp_upload_path).await
        && remove_err.kind() != std::io::ErrorKind::NotFound
    {
        tracing::debug!(
            "Failed to remove malformed upload {}: {}",
            temp_upload_path.display(),
            remove_err
        );
    }
    state.record_job_rejected().await;
    (
        StatusCode::BAD_REQUEST,
        Html(format!(
            r#"<div id="job-{job_id}" class="status-box history-item status-error">
                Invalid multipart upload data
            </div>"#
        )),
    )
}

async fn stage_upload_field(
    mut field: axum::extract::multipart::Field<'_>,
    path: &std::path::Path,
) -> Result<(), std::io::Error> {
    let mut file = File::create(path).await?;
    while let Some(chunk) = field.chunk().await.map_err(std::io::Error::other)? {
        file.write_all(&chunk).await?;
    }
    file.flush().await
}

pub async fn process(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    ReadSignals(signals): ReadSignals<UploadProcessSignals>,
) -> impl IntoResponse {
    // Use the signal job_id to override the job.id created in this function
    let job_id = signals.file_upload.job_id;

    let (tx, rx) = mpsc::channel(64);
    match state.resolve_identity(&headers) {
        Ok(identity) => {
            let mut signals = signals;
            if signals.metadata.account.is_none() {
                signals.metadata.account = identity.account;
            }
            let request_user = identity.user;
            tokio::spawn(async move {
                run_upload_job(state, signals, job_id, request_user, tx).await;
            });
        }
        Err(err) => {
            tokio::spawn(async move {
                state.record_job_rejected().await;
                send_event(
                    &tx,
                    replace_job_event(
                        job_id,
                        template::JobFailed {
                            job_id,
                            error: &format!("{}: {err}", super::identity_failure(&err).1),
                            source: "User submission",
                        },
                    ),
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

pub(super) async fn run_upload_job(
    state: Arc<ServerState>,
    signals: UploadProcessSignals,
    job_id: u64,
    request_user: String,
    tx: mpsc::Sender<ServerEvent>,
) {
    if let Err(err) = super::ensure_active_output_ready(&state).await {
        send_event(
            &tx,
            replace_job_event(
                job_id,
                template::JobFailed {
                    job_id,
                    error: &err,
                    source: "output target",
                },
            ),
        )
        .await;
        send_terminal_signal(&tx, &state).await;
        return;
    }

    send_event(&tx, signal_event(r#"{"processing":true}"#)).await;
    let job = match state.pop_job_request_for_owner(job_id, &request_user).await {
        Some(job) => job,
        None => {
            state.record_job_rejected().await;
            send_event(
                &tx,
                replace_job_event(
                    job_id,
                    template::JobFailed {
                        job_id,
                        error: "Failed to submit file",
                        source: "User submission",
                    },
                ),
            )
            .await;
            send_terminal_signal(&tx, &state).await;
            return;
        }
    };
    job_runner::run_job(state, signals.into(), job_id, request_user, tx, job, true).await;
}

#[cfg(test)]
mod tests {
    use super::{parse_scrubbed_checkbox_value, run_upload_job, send_terminal_signal, submit};
    use crate::server::{JobInput, ServerEvent, ServerState, UploadProcessSignals, test_server_state};
    use axum::{
        body::{Body, to_bytes},
        extract::{FromRequest, Multipart, State},
        http::{HeaderMap, Request, StatusCode},
        response::IntoResponse,
    };
    use std::{path::PathBuf, sync::Arc};
    use tokio::sync::mpsc;

    #[test]
    fn parse_scrubbed_checkbox_value_accepts_truthy_values() {
        for value in ["true", "TRUE", " True ", "1", " on "] {
            assert!(
                parse_scrubbed_checkbox_value(value),
                "expected {value:?} to enable scrub mode"
            );
        }
    }

    #[test]
    fn parse_scrubbed_checkbox_value_rejects_falsy_or_unknown_values() {
        for value in ["", "false", "0", "off", "yes", "scrubbed"] {
            assert!(
                !parse_scrubbed_checkbox_value(value),
                "expected {value:?} to disable scrub mode"
            );
        }
    }

    const FILE_PART: &str = concat!(
        "--upload-boundary\r\n",
        "Content-Disposition: form-data; name=\"file\"; filename=\"diagnostic.zip\"\r\n",
        "Content-Type: application/zip\r\n\r\n",
        "zip-bytes\r\n",
    );

    async fn submit_multipart(body: &str) -> (Arc<ServerState>, StatusCode, u64, String) {
        submit_request(test_server_state(), Body::from(body.to_owned()), HeaderMap::new()).await
    }

    async fn submit_request(
        state: Arc<ServerState>,
        body: Body,
        headers: HeaderMap,
    ) -> (Arc<ServerState>, StatusCode, u64, String) {
        let request = Request::builder()
            .header("content-type", "multipart/form-data; boundary=upload-boundary")
            .body(body)
            .expect("multipart request");
        let multipart = Multipart::from_request(request, &state)
            .await
            .expect("multipart extractor");
        let response = submit(State(state.clone()), headers, multipart).await.into_response();
        let status = response.status();
        let html = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("response body");
        let html = std::str::from_utf8(&html).expect("response HTML");
        let job_id = html
            .split_once("id=\"job-")
            .expect("job response element")
            .1
            .split_once('"')
            .expect("job element identifier")
            .0
            .parse()
            .expect("job ID");
        (state, status, job_id, html.to_owned())
    }

    async fn assert_rejected_upload(body: &str) {
        let (state, status, job_id, _) = submit_multipart(body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            state.job_requests.read().await.is_empty(),
            "rejected upload must not be queued"
        );
        let prefix = format!("esdiag-upload-{job_id}-");
        let mut entries = tokio::fs::read_dir(std::env::temp_dir())
            .await
            .expect("upload staging directory");
        while let Some(entry) = entries.next_entry().await.expect("staging directory entry") {
            assert!(
                !entry.file_name().to_string_lossy().starts_with(&prefix),
                "rejected upload left staged file {}",
                entry.path().display()
            );
        }
    }

    #[tokio::test]
    async fn scrubbed_field_accepts_value_at_byte_limit() {
        let value = format!("{}1", " ".repeat(63));
        let body = format!(
            "{FILE_PART}--upload-boundary\r\nContent-Disposition: form-data; name=\"scrubbed\"\r\n\r\n{value}\r\n--upload-boundary--\r\n"
        );
        let (state, status, job_id, _) = submit_multipart(&body).await;
        let job = state.pop_job_request(job_id).await.expect("accepted upload");
        let JobInput::LocalArchive {
            path,
            scrubbed_override,
            ..
        } = &job.input
        else {
            panic!("expected local archive");
        };
        let bytes = tokio::fs::read(path).await.expect("queued staged bytes");
        job.cleanup().await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(*scrubbed_override, Some(true));
        assert_eq!(bytes, b"zip-bytes");
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn scrubbed_field_rejects_oversized_chunks_before_or_after_staging() {
        for file_first in [true, false] {
            let value = format!("{}1", " ".repeat(64));
            let checkbox =
                format!("--upload-boundary\r\nContent-Disposition: form-data; name=\"scrubbed\"\r\n\r\n{value}\r\n");
            let body = if file_first {
                format!("{FILE_PART}{checkbox}--upload-boundary--\r\n")
            } else {
                format!("{checkbox}{FILE_PART}--upload-boundary--\r\n")
            };
            let bytes = bytes::Bytes::from(body);
            let chunks = (0..bytes.len())
                .step_by(8)
                .map(move |start| Ok::<_, std::io::Error>(bytes.slice(start..(start + 8).min(bytes.len()))));
            let (state, status, job_id, html) = submit_request(
                test_server_state(),
                Body::from_stream(futures::stream::iter(chunks)),
                HeaderMap::new(),
            )
            .await;
            if status != StatusCode::BAD_REQUEST {
                state.discard_job_request(job_id).await;
            }
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert!(html.contains("Invalid multipart upload data"));
            assert!(state.job_requests.read().await.is_empty());
            let stats = state.get_stats().await;
            assert_eq!((stats.jobs.total, stats.jobs.failed), (1, 1));
            let prefix = format!("esdiag-upload-{job_id}-");
            let mut entries = tokio::fs::read_dir(std::env::temp_dir())
                .await
                .expect("staging directory");
            while let Some(entry) = entries.next_entry().await.expect("staging entry") {
                assert!(!entry.file_name().to_string_lossy().starts_with(&prefix));
            }
        }
    }

    #[tokio::test]
    async fn submit_rejects_truncated_scrubbed_field_and_cleans_staged_file() {
        let body =
            format!("{FILE_PART}--upload-boundary\r\nContent-Disposition: form-data; name=\"scrubbed\"\r\n\r\nfalse");
        assert_rejected_upload(&body).await;
    }

    #[tokio::test]
    async fn submit_rejects_malformed_trailing_part_and_cleans_staged_file() {
        let body = format!("{FILE_PART}--upload-boundary\r\ninvalid-header\r\n\r\nvalue\r\n--upload-boundary--\r\n");
        assert_rejected_upload(&body).await;
    }

    #[tokio::test]
    async fn submit_rejects_truncated_ignored_field_and_cleans_staged_file() {
        let body =
            format!("{FILE_PART}--upload-boundary\r\nContent-Disposition: form-data; name=\"other\"\r\n\r\nvalue");
        assert_rejected_upload(&body).await;
    }

    #[tokio::test]
    async fn submit_rejects_truncated_file_body_and_cleans_partial_file() {
        assert_rejected_upload(FILE_PART).await;
    }

    #[tokio::test]
    async fn submit_rejects_duplicate_files_and_cleans_staged_file() {
        let body = format!("{FILE_PART}{FILE_PART}--upload-boundary--\r\n");
        assert_rejected_upload(&body).await;
    }

    #[tokio::test]
    async fn submit_stages_valid_upload_with_explicit_false_override() {
        let body = format!(
            "{FILE_PART}--upload-boundary\r\nContent-Disposition: form-data; name=\"scrubbed\"\r\n\r\nfalse\r\n--upload-boundary--\r\n"
        );
        let (state, status, job_id, _) = submit_multipart(&body).await;
        assert_eq!(status, StatusCode::OK);
        let job = state.pop_job_request(job_id).await.expect("staged upload");
        let JobInput::LocalArchive {
            path,
            scrubbed_override,
            ..
        } = &job.input
        else {
            panic!("expected local upload archive");
        };
        assert_eq!(*scrubbed_override, Some(false));
        assert_eq!(tokio::fs::read(path).await.expect("staged file"), b"zip-bytes");
        job.cleanup().await;
    }

    #[tokio::test]
    async fn submit_escapes_filename_html_without_changing_staged_name() {
        let filename = "<img src=x onerror=alert(1)>&'scrubbed.zip";
        let body = format!(
            "{}--upload-boundary--\r\n",
            FILE_PART.replace("diagnostic.zip", filename)
        );
        let (state, status, job_id, html) = submit_multipart(&body).await;
        assert_eq!(status, StatusCode::OK);
        let job = state.pop_job_request(job_id).await.expect("staged upload");
        job.cleanup().await;
        let JobInput::LocalArchive {
            filename: staged_name, ..
        } = &job.input
        else {
            panic!("expected local upload archive");
        };
        assert_eq!(staged_name, filename);
        assert!(!html.contains("<img"), "filename must not create an HTML element");
        assert!(html.contains("&#60;img src=x onerror=alert(1)&#62;&#38;&#39;scrubbed.zip"));
    }

    #[tokio::test]
    async fn run_upload_job_missing_upload_emits_failure_and_terminal_signal() {
        let state = test_server_state();
        let signals = UploadProcessSignals::default();
        let (tx, mut rx) = mpsc::channel(8);

        run_upload_job(state, signals, 42, "Anonymous".to_string(), tx).await;

        let mut saw_failure = false;
        let mut saw_terminal = false;
        while let Some(event) = rx.recv().await {
            match event {
                ServerEvent::Template { html, .. } if html.contains("Failed to submit file") => {
                    saw_failure = true;
                }
                ServerEvent::ReplaceSelector { selector, html, .. }
                    if selector == "#job-42" && html.contains("Failed to submit file") =>
                {
                    saw_failure = true;
                }
                ServerEvent::Signals { payload, .. } if payload.contains(r#""processing":false"#) => {
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
            ServerEvent::Signals { payload, .. } => {
                assert!(payload.contains(r#""file_upload":{"job_id":0}"#));
            }
            _ => panic!("expected terminal signal payload"),
        }
    }

    #[tokio::test]
    async fn upload_job_cannot_be_claimed_by_different_owner() {
        let state = test_server_state();
        state
            .push_upload(
                7,
                "alice@example.com".to_string(),
                Some("accounts.google.com".to_string()),
                "upload.zip".to_string(),
                PathBuf::from("/tmp/nonexistent-upload.zip"),
                None,
            )
            .await;
        let signals = UploadProcessSignals::default();
        let (tx, mut rx) = mpsc::channel(8);

        run_upload_job(state.clone(), signals, 7, "bob@example.com".to_string(), tx).await;

        let mut saw_failure = false;
        while let Some(event) = rx.recv().await {
            match event {
                ServerEvent::ReplaceSelector { html, .. } if html.contains("Failed to submit file") => {
                    saw_failure = true;
                }
                _ => {}
            }
        }

        assert!(saw_failure, "expected owner mismatch failure");
        assert!(state.pop_job_request_for_owner(7, "alice@example.com").await.is_some());
    }

    #[tokio::test]
    async fn run_upload_job_completes_when_client_disconnected() {
        let state = test_server_state();
        let signals = UploadProcessSignals::default();
        let (tx, rx) = mpsc::channel(1);
        drop(rx);

        run_upload_job(state, signals, 999, "Anonymous".to_string(), tx).await;
    }
}
