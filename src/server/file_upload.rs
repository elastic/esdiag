// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    CleanupMode, ServerEvent, ServerState, UploadProcessSignals, cleanup_input_path, job_runner, receiver_stream,
    replace_job_event, signal_event, template,
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
            tracing::warn!("Upload submit denied: {}", err);
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
                    cleanup_input_path(&temp_upload_path, CleanupMode::FileOnly, "duplicate upload").await;
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
                        cleanup_input_path(&temp_upload_path, CleanupMode::FileOnly, "partial upload").await;
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
                match std::str::from_utf8(&value[..length]) {
                    Ok(value) => scrubbed_override = Some(parse_scrubbed_checkbox_value(value)),
                    Err(err) => return reject_malformed_upload(&state, job_id, staged_upload, err).await,
                }
            }
            _ => {}
        }
    }

    if let Some((filename, temp_upload_path)) = staged_upload {
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
    if let Some((_filename, temp_upload_path)) = staged_upload {
        cleanup_input_path(&temp_upload_path, CleanupMode::FileOnly, "malformed upload").await;
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
                            source: "User upload",
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
                        error: "Failed to upload file",
                        source: "User upload",
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
    use super::{parse_scrubbed_checkbox_value, run_upload_job, submit};
    use crate::server::{
        JobInput, ProcessMode, RuntimeMode, SendMode, ServerEvent, ServerPolicy, ServerState, UploadProcessSignals,
        job_runner, test_server_state,
    };
    use axum::{
        body::{Body, to_bytes},
        extract::{FromRequest, Multipart, State},
        http::{HeaderMap, Request, StatusCode},
        response::IntoResponse,
    };
    use std::{sync::Arc, time::Duration};
    use tokio::{sync::mpsc, time::advance};
    #[cfg(unix)]
    use uuid::Uuid;

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

    async fn assert_rejected_upload(body: &str, reason: &str) {
        let (state, status, job_id, html) = submit_multipart(body).await;
        assert!(html.contains(reason), "expected rejection reason {reason:?}: {html}");
        let stats = state.get_stats().await;
        assert_eq!(stats.jobs.total, 1);
        assert_eq!(stats.jobs.failed, 1);
        assert_eq!(stats.jobs.success, 0);
        assert_eq!(stats.jobs.active, 0);
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
            let (state, status, job_id, _) = submit_request(
                test_server_state(),
                Body::from_stream(futures::stream::iter(chunks)),
                HeaderMap::new(),
            )
            .await;
            if status != StatusCode::BAD_REQUEST {
                state.discard_job_request(job_id).await;
            }
            assert_eq!(status, StatusCode::BAD_REQUEST);
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
    async fn scrubbed_field_rejects_invalid_utf8_before_or_after_staging() {
        for file_first in [true, false] {
            let checkbox_head = b"--upload-boundary\r\nContent-Disposition: form-data; name=\"scrubbed\"\r\n\r\n";
            let parts: [&'static [u8]; 5] = if file_first {
                [
                    FILE_PART.as_bytes(),
                    checkbox_head,
                    b"\xff",
                    b"\r\n",
                    b"--upload-boundary--\r\n",
                ]
            } else {
                [
                    checkbox_head,
                    b"\xff",
                    b"\r\n",
                    FILE_PART.as_bytes(),
                    b"--upload-boundary--\r\n",
                ]
            };
            let chunks = parts
                .into_iter()
                .map(|part| Ok::<_, std::io::Error>(bytes::Bytes::from_static(part)));
            let (state, status, job_id, _) = submit_request(
                test_server_state(),
                Body::from_stream(futures::stream::iter(chunks)),
                HeaderMap::new(),
            )
            .await;
            if status != StatusCode::BAD_REQUEST {
                state.discard_job_request(job_id).await;
            }
            assert_eq!(status, StatusCode::BAD_REQUEST);
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
        assert_rejected_upload(&body, "Invalid multipart upload data").await;
    }

    #[tokio::test]
    async fn submit_rejects_malformed_trailing_part_and_cleans_staged_file() {
        let body = format!("{FILE_PART}--upload-boundary\r\ninvalid-header\r\n\r\nvalue\r\n--upload-boundary--\r\n");
        assert_rejected_upload(&body, "Invalid multipart upload data").await;
    }

    #[tokio::test]
    async fn submit_rejects_truncated_ignored_field_and_cleans_staged_file() {
        let body =
            format!("{FILE_PART}--upload-boundary\r\nContent-Disposition: form-data; name=\"other\"\r\n\r\nvalue");
        assert_rejected_upload(&body, "Invalid multipart upload data").await;
    }

    #[tokio::test]
    async fn submit_rejects_truncated_file_body_and_cleans_partial_file() {
        assert_rejected_upload(FILE_PART, "Failed to stage upload data:").await;
    }

    #[tokio::test]
    async fn submit_rejects_duplicate_files_and_cleans_staged_file() {
        let body = format!("{FILE_PART}{FILE_PART}--upload-boundary--\r\n");
        assert_rejected_upload(&body, "Multiple upload files are not supported").await;
    }

    #[tokio::test]
    async fn submit_preserves_missing_and_invalid_file_rejections() {
        assert_rejected_upload("--upload-boundary--\r\n", "Missing upload file").await;
        assert_rejected_upload(
            "--upload-boundary\r\nContent-Disposition: form-data; name=\"file\"\r\n\r\nbytes\r\n--upload-boundary--\r\n",
            "Missing file name",
        )
        .await;
        assert_rejected_upload(
            &format!(
                "{}--upload-boundary--\r\n",
                FILE_PART.replace("diagnostic.zip", "diagnostic.txt")
            ),
            "Invalid file type, only .zip files are allowed.",
        )
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn malformed_trailing_part_keeps_directory_replacing_unqueued_staged_file() {
        let marker = Uuid::new_v4().to_string();
        let first_part = format!(
            "--upload-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"diagnostic.zip\"\r\n\r\n{marker}\r\n--upload-boundary\r\n"
        );
        let (sender, mut receiver) = mpsc::channel::<String>(2);
        let stream = async_stream::stream! {
            while let Some(chunk) = receiver.recv().await {
                yield Ok::<_, std::io::Error>(bytes::Bytes::from(chunk));
            }
        };
        let state = test_server_state();
        let request_state = state.clone();
        let request =
            tokio::spawn(
                async move { submit_request(request_state, Body::from_stream(stream), HeaderMap::new()).await },
            );
        sender.send(first_part).await.expect("first multipart segment");
        let staged = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let mut entries = tokio::fs::read_dir(std::env::temp_dir())
                    .await
                    .expect("temporary upload directory");
                while let Some(entry) = entries.next_entry().await.expect("upload entry") {
                    if entry.file_name().to_string_lossy().starts_with("esdiag-upload-")
                        && tokio::fs::read(entry.path()).await.ok().as_deref() == Some(marker.as_bytes())
                    {
                        return entry.path();
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("first file staged before completing multipart request");
        let saved = tempfile::tempdir().expect("preserve the staged bytes");
        let moved = saved.path().join("original.zip");
        tokio::fs::rename(&staged, &moved).await.expect("move staged file");
        tokio::fs::create_dir(&staged)
            .await
            .expect("replace staged file with directory");
        sender
            .send("invalid-header\r\n\r\nvalue\r\n--upload-boundary--\r\n".to_string())
            .await
            .expect("malformed trailing field");
        drop(sender);
        let (_, status, job_id, html) = request.await.expect("upload request");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(html.contains("Invalid multipart upload data"));
        assert!(
            staged.is_dir(),
            "file-only rejection must not recursively remove a directory"
        );
        assert_eq!(
            tokio::fs::read(&moved).await.expect("preserved staged data"),
            marker.as_bytes()
        );
        assert!(state.job_requests.read().await.is_empty());
        let stats = state.get_stats().await;
        assert_eq!((stats.jobs.total, stats.jobs.failed), (1, 1));
        assert!(
            staged
                .file_name()
                .expect("upload filename")
                .to_string_lossy()
                .starts_with(&format!("esdiag-upload-{job_id}-"))
        );
        tokio::fs::remove_dir(&staged)
            .await
            .expect("remove substituted directory");
    }

    #[tokio::test]
    async fn submit_keeps_valid_staged_bytes_and_scrub_selection_until_claim() {
        for (checkbox, expected_override) in [(false, None), (true, Some(false))] {
            let filename = "diagnostic-scrubbed.zip";
            let body = format!(
                "{}--upload-boundary\r\nContent-Disposition: form-data; name=\"other\"\r\n\r\nignored\r\n{}--upload-boundary--\r\n",
                FILE_PART.replace("diagnostic.zip", filename),
                if checkbox {
                    "--upload-boundary\r\nContent-Disposition: form-data; name=\"scrubbed\"\r\n\r\nfalse\r\n"
                } else {
                    ""
                },
            );
            let (state, status, job_id, html) = submit_multipart(&body).await;
            assert_eq!(status, StatusCode::OK);
            assert!(html.contains(filename));
            assert_eq!(state.job_requests.read().await.len(), 1);
            let stats = state.get_stats().await;
            assert_eq!(stats.jobs.total, 0);
            assert_eq!(stats.jobs.failed, 0);
            let job = state.pop_job_request(job_id).await.expect("staged upload");
            let JobInput::LocalArchive {
                filename: staged_name,
                path,
                cleanup_path,
                scrubbed_override,
                ..
            } = &job.input
            else {
                panic!("expected local upload archive");
            };
            assert_eq!(staged_name, filename);
            assert_eq!(*scrubbed_override, expected_override);
            assert_eq!(cleanup_path.as_ref(), Some(path));
            assert_eq!(tokio::fs::read(path).await.expect("queued staged file"), b"zip-bytes");
            job.cleanup().await;
            assert!(!path.exists(), "claimed upload cleanup must remove staged bytes");
        }
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
        let JobInput::LocalArchive {
            filename: staged_name,
            path,
            ..
        } = &job.input
        else {
            panic!("expected local upload archive");
        };
        assert_eq!(staged_name, filename);
        assert_eq!(tokio::fs::read(path).await.expect("staged file"), b"zip-bytes");
        assert!(!html.contains("<img"), "filename must not create an HTML element");
        assert!(html.contains("&#60;img src=x onerror=alert(1)&#62;&#38;&#39;scrubbed.zip"));
        job.cleanup().await;
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn missing_upload_runner_targets_failure_and_resets_processing_state() {
        let mut state = test_server_state();
        let settings = Arc::get_mut(&mut state).expect("unique server state");
        settings.runtime_mode = RuntimeMode::Service;
        settings.server_policy = ServerPolicy::defaults(RuntimeMode::Service);
        let signals = UploadProcessSignals::default();
        let (tx, mut rx) = mpsc::channel(8);

        run_upload_job(state.clone(), signals, 42, "Anonymous".to_string(), tx).await;

        let mut saw_failure = false;
        let mut saw_terminal = false;
        while let Some(event) = rx.recv().await {
            match event {
                ServerEvent::ReplaceSelector { selector, html, .. } => {
                    assert_eq!(selector, "#job-42");
                    assert!(html.contains("status-error"), "missing upload receives a failure card");
                    saw_failure = true;
                }
                ServerEvent::Signals { payload, .. } => {
                    let signals: serde_json::Value = serde_json::from_str(&payload).expect("signal JSON");
                    if signals.get("processing").and_then(serde_json::Value::as_bool) == Some(false) {
                        assert_eq!(
                            signals
                                .pointer("/file_upload/job_id")
                                .and_then(serde_json::Value::as_u64),
                            Some(0),
                        );
                        saw_terminal = true;
                    }
                }
                _ => {}
            }
        }

        assert!(saw_failure, "missing upload receives a targeted failure");
        assert!(saw_terminal, "missing upload resets processing and upload selection");
        let stats = state.get_stats().await;
        assert_eq!(stats.jobs.total, 1);
        assert_eq!(stats.jobs.failed, 1);
        assert_eq!(stats.jobs.success, 0);
        assert_eq!(stats.jobs.active, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn wrong_owner_cannot_claim_upload_and_claimed_input_survives_expiry_until_runner_cleanup() {
        let mut state = test_server_state();
        let settings = Arc::get_mut(&mut state).expect("unique server state");
        settings.runtime_mode = RuntimeMode::Service;
        settings.server_policy = ServerPolicy::defaults(RuntimeMode::Service);
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Goog-Authenticated-User-Email",
            "accounts.google.com:alice@example.com".parse().expect("valid identity"),
        );
        let (state, status, job_id, _) = submit_request(
            state,
            Body::from(format!("{FILE_PART}--upload-boundary--\r\n")),
            headers,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let path = match &state
            .job_requests
            .read()
            .await
            .get(&job_id)
            .expect("queued upload")
            .input
        {
            JobInput::LocalArchive { path, .. } => path.clone(),
            _ => panic!("expected staged upload"),
        };
        tokio::task::yield_now().await;
        assert!(
            state
                .pop_job_request_for_owner(job_id, "bob@example.com")
                .await
                .is_none(),
            "another owner cannot claim the staged upload"
        );
        assert_eq!(
            tokio::fs::read(&path).await.expect("wrong owner left file"),
            b"zip-bytes"
        );
        assert_eq!(state.job_requests.read().await.len(), 1);
        let job = state
            .pop_job_request_for_owner(job_id, "alice@example.com")
            .await
            .expect("owner claims upload");
        assert_eq!(job.owner, "alice@example.com");
        advance(Duration::from_secs(300)).await;
        tokio::task::yield_now().await;
        assert!(state.job_requests.read().await.is_empty());
        assert_eq!(
            tokio::fs::read(&path).await.expect("claimed file survives expiry"),
            b"zip-bytes"
        );

        let mut signals = UploadProcessSignals::default();
        signals.job.send.mode = SendMode::Local;
        signals.job.process.mode = ProcessMode::Forward;
        let (tx, rx) = mpsc::channel(8);
        drop(rx);
        job_runner::run_job(
            state.clone(),
            signals.into(),
            job_id,
            "alice@example.com".to_string(),
            tx,
            job,
            true,
        )
        .await;
        assert!(!path.exists(), "runner validation rejection removes claimed input");
        let stats = state.get_stats().await;
        assert_eq!(stats.jobs.total, 1, "runner validation rejection alone");
        assert_eq!(stats.jobs.failed, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn unclaimed_upload_expires_and_removes_staged_file_at_five_minutes() {
        let (state, status, job_id, _) = submit_multipart(&format!("{FILE_PART}--upload-boundary--\r\n")).await;
        assert_eq!(status, StatusCode::OK);
        let path = match &state
            .job_requests
            .read()
            .await
            .get(&job_id)
            .expect("queued upload")
            .input
        {
            JobInput::LocalArchive { path, .. } => path.clone(),
            _ => panic!("expected staged upload"),
        };
        tokio::task::yield_now().await;
        advance(Duration::from_secs(299)).await;
        assert_eq!(tokio::fs::read(&path).await.expect("not yet expired"), b"zip-bytes");
        assert_eq!(state.job_requests.read().await.len(), 1);
        advance(Duration::from_secs(1)).await;
        tokio::time::timeout(Duration::from_secs(1), async {
            while !state.job_requests.read().await.is_empty() || path.exists() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("unclaimed expiry task completed cleanup");
        assert!(!path.exists(), "unclaimed staged file expires");
        let stats = state.get_stats().await;
        assert_eq!(stats.jobs.total, 0);
        assert_eq!(stats.jobs.failed, 0);
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
