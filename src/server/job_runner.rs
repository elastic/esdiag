// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    CollectSource, JobAdmissionError, JobInput, JobRequest, JobRunSignals, ProcessMode, SendMode, ServerEvent,
    ServerState, job_feed_event, replace_job_event, signal_event, template, template_event,
};
use crate::{
    data::{Application, HostRole, Uri},
    exporter::{DocumentExporter, Exporter},
    job::{
        context::{ExecutionContext, ExecutionIdentity, ExecutionObserver, RetentionPolicy},
        executor::execute_with_context,
        model::{BindingKey, ExportTarget, Input, Job, Process, SaveTarget, SendTarget},
        outcome::{ExecutionEvent, ExecutionOutcome},
    },
    processor::{
        DiagnosticOutcome, EventSeverity, Identifiers, IncludedDiagnosticJobEvent, SkipKind,
        api::{ApiResolver, ProcessSelection},
        display_label,
    },
    receiver::Receiver,
};
use eyre::{Result, eyre};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{fs, sync::mpsc};
#[cfg(test)]
use tokio::{fs::File, io::AsyncWriteExt};

const RETAINED_BUNDLE_TTL: Duration = Duration::from_secs(3600);

struct WebExecutionObserver {
    tx: mpsc::Sender<ServerEvent>,
    owner: String,
}

impl ExecutionObserver for WebExecutionObserver {
    fn observe(&self, event: &ExecutionEvent) {
        let payload = serde_json::json!({
            "execution": {
                "jobId": event.identity.job_id,
                "stage": format!("{:?}", event.stage).to_ascii_lowercase(),
                "lifecycle": format!("{:?}", event.lifecycle).to_ascii_lowercase(),
                "message": event.message,
            }
        });
        let _ = self
            .tx
            .try_send(signal_event(payload.to_string()).for_owner(self.owner.clone()));
    }
}

pub async fn run_job(
    state: Arc<ServerState>,
    signals: JobRunSignals,
    job_id: u64,
    request_user: String,
    tx: mpsc::Sender<ServerEvent>,
    job: JobRequest,
    replace_existing_entry: bool,
) {
    let source = job.source().to_string();
    let owner = job.owner.clone();
    let started = Arc::new(AtomicBool::new(false));
    let download_token = signals.archive.download_token.trim().to_string();
    let should_track_download = (signals.job.collect.save || signals.job.send.raw_local) && !download_token.is_empty();
    let validation = validate_job_request(&state, &signals, &job).await;
    if let Err(error) = validation {
        if should_track_download {
            state
                .reject_retained_bundle(&download_token, &request_user, error.to_string(), RETAINED_BUNDLE_TTL)
                .await;
        }
        state.record_job_rejected().await;
        send_event(
            &tx,
            terminal_job_event(
                replace_existing_entry,
                job_id,
                template::JobFailed {
                    job_id,
                    error: &error.to_string(),
                    source: &source,
                },
            ),
        )
        .await;
        send_terminal_signal(&tx, &state).await;
        job.cleanup().await;
        return;
    }

    if should_track_download {
        state
            .accept_retained_bundle(&download_token, &request_user, RETAINED_BUNDLE_TTL)
            .await;
        state.schedule_retained_bundle_cleanup(download_token.clone(), RETAINED_BUNDLE_TTL);
    }

    let identifiers = merged_identifiers(
        job.identifiers.clone(),
        signals.metadata.clone(),
        request_user.clone(),
        &job.input,
    );
    let setup_inserts_processing_entry = matches!(&job.input, JobInput::FromRemoteHost { .. })
        && signals.job.process.mode == ProcessMode::Process
        && !signals.job.collect.save
        && !replace_existing_entry;
    let result = execute_unified_web_job(
        state.clone(),
        &signals,
        job_id,
        &owner,
        &source,
        identifiers,
        &tx,
        &job,
        replace_existing_entry,
        started.clone(),
    )
    .await;

    if let Err(error) = result {
        if should_track_download {
            let token_has_bundle = state
                .retained_bundle(&download_token)
                .await
                .and_then(|bundle| bundle.path)
                .is_some();
            if !token_has_bundle {
                state
                    .reject_retained_bundle(&download_token, &request_user, error.to_string(), RETAINED_BUNDLE_TTL)
                    .await;
            }
        }
        if is_job_admission_error(&error) || !started.load(Ordering::SeqCst) {
            state.record_job_rejected().await;
        } else {
            state.record_failure(&owner).await;
        }
        send_event(
            &tx,
            terminal_job_event(
                replace_existing_entry || setup_inserts_processing_entry,
                job_id,
                template::JobFailed {
                    job_id,
                    error: &error.to_string(),
                    source: &source,
                },
            ),
        )
        .await;
    }

    send_terminal_signal(&tx, &state).await;
    job.cleanup().await;
}

#[allow(clippy::too_many_arguments)]
async fn execute_unified_web_job(
    state: Arc<ServerState>,
    signals: &JobRunSignals,
    job_id: u64,
    owner: &str,
    source: &str,
    identifiers: Identifiers,
    tx: &mpsc::Sender<ServerEvent>,
    request: &JobRequest,
    replace_existing_entry: bool,
    started: Arc<AtomicBool>,
) -> Result<()> {
    let mut draft = signals.job.clone();
    draft.normalize_targets();
    let mut context = ExecutionContext::default()
        .with_identity(ExecutionIdentity::new(job_id, owner))
        .with_observer(Arc::new(WebExecutionObserver {
            tx: tx.clone(),
            owner: owner.to_string(),
        }));

    let input = match &request.input {
        JobInput::LocalArchive { path, .. } => Input::Load {
            uri: Uri::File(path.clone()),
        },
        JobInput::FromServiceLink { uri, .. } => {
            let binding = BindingKey::try_new(format!("web-service-link-{job_id}"))?;
            context.inputs.bind_uri(binding.clone(), uri.clone(), None);
            Input::LoadBinding { binding }
        }
        JobInput::FromRemoteHost {
            host, diagnostic_type, ..
        } => {
            let binding = BindingKey::try_new(format!("web-collect-{job_id}"))?;
            let resolved = host.clone().resolve()?;
            let application = resolved.application();
            context.inputs.bind_receiver(
                binding.clone(),
                Receiver::try_from(resolved.into_known_host())?,
                Some(application),
            );
            Input::CollectBinding {
                binding,
                diagnostic_type: diagnostic_type.clone(),
                include: None,
                exclude: None,
            }
        }
    };

    let send = raw_send_target(&draft);
    let save = if input.is_collect() && (draft.collect.save || send.is_some()) {
        if draft.collect.save {
            let output_dir = std::env::temp_dir().join(format!("esdiag-web-job-{job_id}"));
            Some(SaveTarget::retained(output_dir))
        } else {
            Some(SaveTarget::temporary())
        }
    } else {
        None
    };

    if !input.is_collect() && (draft.collect.save || draft.send.raw_local) {
        context = context.with_retention(RetentionPolicy::RetainLoadedBundle);
    }

    state.record_job_started(owner).await?;
    started.store(true, Ordering::SeqCst);
    let processing_template = if draft.process.mode == ProcessMode::Process {
        processing_job_event(
            replace_existing_entry,
            job_id,
            template::JobProcessing { job_id, source },
        )
    } else {
        processing_job_event(
            replace_existing_entry,
            job_id,
            template::JobForwardProcessing { job_id, source },
        )
    };
    send_event(tx, processing_template).await;
    send_event(tx, signal_event(r#"{"loading":false,"processing":true}"#)).await;

    let process = if draft.process.enabled && draft.process.mode == ProcessMode::Process {
        let exporter = select_processed_exporter(state.clone(), signals).await?;
        let binding = BindingKey::try_new(format!("web-document-export-{job_id}"))?;
        context.bind_document_exporter(binding.clone(), DocumentExporter::try_from(exporter)?);
        Some(Process {
            selection: explicit_process_selection(signals)?,
            export: ExportTarget::Binding { binding },
        })
    } else {
        None
    };

    let job = Job::try_new(identifiers, input, save, process, send)?;
    let outcome = execute_with_context(job, context).await;
    let execution_error = (!outcome.succeeded()).then(|| execution_outcome_error(&outcome));

    if let Some(path) = outcome.retained_bundle.as_ref()
        && (draft.collect.save || draft.send.raw_local)
    {
        let (retained_path, cleanup_path) = match &request.input {
            JobInput::LocalArchive {
                cleanup_path: Some(_), ..
            } => {
                let retained_dir = std::env::temp_dir().join(format!("esdiag-retained-{job_id}"));
                fs::create_dir_all(&retained_dir).await?;
                let retained_path = retained_dir.join(
                    path.file_name()
                        .unwrap_or_else(|| std::ffi::OsStr::new("diagnostic.zip")),
                );
                fs::copy(path, &retained_path).await?;
                (retained_path, Some(retained_dir))
            }
            JobInput::LocalArchive { cleanup_path: None, .. } => (path.clone(), None),
            JobInput::FromServiceLink { .. } | JobInput::FromRemoteHost { .. } => {
                (path.clone(), path.parent().map(Path::to_path_buf))
            }
        };
        let filename = retained_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("diagnostic.zip")
            .to_string();
        publish_retained_download(
            &state,
            owner,
            &signals.archive.download_token,
            filename,
            retained_path,
            cleanup_path,
        )
        .await?;
    }

    if let Some(report) = outcome.report.as_ref() {
        let diagnostic_outcome = report.outcome();
        if execution_error.is_some() {
            state.record_failure(owner).await;
        } else {
            state
                .record_outcome(owner, diagnostic_outcome, report.diagnostic.docs.errors)
                .await;
        }
        started.store(false, Ordering::SeqCst);
        let product = report.diagnostic.display_label();
        let upload_destination = outcome
            .upload
            .as_ref()
            .map(|upload| format!("https://upload.elastic.co/g/{}", upload.slug));
        let (status_class, heading) = if execution_error.is_some() {
            ("status-error", "⚠️ Diagnostic completed with output failures")
        } else {
            (
                completed_status_class(&diagnostic_outcome),
                completed_heading(&diagnostic_outcome),
            )
        };
        let recorded_failures = recorded_report_failures(report);
        send_event(
            tx,
            terminal_job_event(
                replace_existing_entry,
                job_id,
                template::JobCompleted {
                    job_id,
                    status_class,
                    heading,
                    diagnostic_id: &report.diagnostic.metadata.id,
                    docs_created: &report.diagnostic.docs.created,
                    duration: &format!("{:.3}", report.diagnostic.processing_duration as f64 / 1000.0),
                    source,
                    kibana_link: report.diagnostic.kibana_link.as_deref().unwrap_or(""),
                    product: &product,
                    outcome: diagnostic_outcome.as_str(),
                    upload_destination: upload_destination.as_deref(),
                    execution_error: execution_error.as_deref(),
                    recorded_failures,
                },
            ),
        )
        .await;
        render_child_outcomes(tx, owner, &outcome).await;
        return Ok(());
    }

    if let Some(mut error) = execution_error {
        if let Some(upload) = outcome.upload.as_ref() {
            error.push_str(&format!(
                "; raw bundle uploaded successfully: https://upload.elastic.co/g/{}",
                upload.slug
            ));
        }
        return Err(eyre!(error));
    }

    state.record_success(owner, 0, 0).await;
    started.store(false, Ordering::SeqCst);
    if let Some(upload) = outcome.upload {
        let destination = format!("https://upload.elastic.co/g/{}", upload.slug);
        send_event(
            tx,
            terminal_job_event(
                replace_existing_entry,
                job_id,
                template::JobForwardCompleted {
                    job_id,
                    source,
                    destination: &destination,
                },
            ),
        )
        .await;
    } else if let Some(path) = outcome.retained_bundle {
        let archive_path = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("diagnostic.zip");
        send_event(
            tx,
            terminal_job_event(
                replace_existing_entry,
                job_id,
                template::JobCollectionCompleted {
                    job_id,
                    source,
                    archive_path,
                },
            ),
        )
        .await;
    }
    Ok(())
}

fn execution_outcome_error(outcome: &ExecutionOutcome) -> String {
    outcome
        .stages
        .iter()
        .filter_map(|stage| match &stage.status {
            crate::job::outcome::StageStatus::Failed(error) => Some(format!("{:?} failed: {error}", stage.stage)),
            crate::job::outcome::StageStatus::Blocked(reason) => Some(format!("{:?} blocked: {reason}", stage.stage)),
            crate::job::outcome::StageStatus::Succeeded | crate::job::outcome::StageStatus::Skipped(_) => None,
        })
        .collect::<Vec<_>>()
        .join("; ")
}

async fn render_child_outcomes(tx: &mpsc::Sender<ServerEvent>, owner: &str, outcome: &ExecutionOutcome) {
    let (child_tx, child_rx) = mpsc::unbounded_channel();
    for child in &outcome.children {
        let _ = child_tx.send(IncludedDiagnosticJobEvent::Queued {
            job_id: child.job_id(),
            path: child.path.clone(),
        });
        let _ = child_tx.send(IncludedDiagnosticJobEvent::Started {
            job_id: child.job_id(),
            path: child.path.clone(),
        });
        let event = match (&child.diagnostic_outcome, child.report()) {
            (DiagnosticOutcome::Skipped(_), _) => IncludedDiagnosticJobEvent::Skipped {
                job_id: child.job_id(),
                path: child.path.clone(),
                outcome: child.diagnostic_outcome,
                application: child.application(),
                platform: child.platform(),
                reason: child.execution_error().unwrap_or_default().to_string(),
            },
            (_, Some(report)) => IncludedDiagnosticJobEvent::Completed {
                job_id: child.job_id(),
                path: child.path.clone(),
                outcome: child.diagnostic_outcome,
                application: report.diagnostic.application,
                platform: report.diagnostic.platform(),
                diagnostic_id: report.diagnostic.metadata.id.clone(),
                docs_created: report.diagnostic.docs.created,
                duration_ms: child.runtime.unwrap_or_default(),
                kibana_link: report.diagnostic.kibana_link.clone(),
                execution_error: child.export_error().map(str::to_string),
                recorded_failures: recorded_report_failures(report),
            },
            (_, None) => IncludedDiagnosticJobEvent::Failed {
                job_id: child.job_id(),
                path: child.path.clone(),
                error: child.execution_error().unwrap_or_default().to_string(),
            },
        };
        let _ = child_tx.send(event);
    }
    drop(child_tx);
    render_child_diagnostic_events(tx.clone(), owner.to_string(), child_rx).await;
}

fn is_job_admission_error(error: &eyre::Report) -> bool {
    error.downcast_ref::<JobAdmissionError>().is_some()
}

fn raw_send_target(draft: &crate::data::JobDraft) -> Option<SendTarget> {
    draft
        .send
        .raw_remote_target
        .clone()
        .or_else(|| {
            (draft.process.mode == ProcessMode::Forward && draft.send.mode == SendMode::Remote)
                .then(|| draft.send.remote_target.clone())
                .flatten()
        })
        .map(|upload_id| SendTarget { upload_id })
}

async fn render_child_diagnostic_events(
    tx: mpsc::Sender<ServerEvent>,
    owner: String,
    mut child_event_rx: mpsc::UnboundedReceiver<IncludedDiagnosticJobEvent>,
) {
    while let Some(event) = child_event_rx.recv().await {
        match event {
            IncludedDiagnosticJobEvent::Queued { job_id, path } => {
                let source = child_source(&path);
                send_event(
                    &tx,
                    job_feed_event(template::JobProcessing {
                        job_id,
                        source: &source,
                    })
                    .for_owner(owner.clone()),
                )
                .await;
            }
            IncludedDiagnosticJobEvent::Started { job_id, path } => {
                let source = child_source(&path);
                send_event(
                    &tx,
                    replace_job_event(
                        job_id,
                        template::JobProcessing {
                            job_id,
                            source: &source,
                        },
                    )
                    .for_owner(owner.clone()),
                )
                .await;
            }
            IncludedDiagnosticJobEvent::Completed {
                job_id,
                path,
                outcome,
                application,
                platform,
                diagnostic_id,
                docs_created,
                duration_ms,
                kibana_link,
                execution_error,
                recorded_failures,
            } => {
                let source = child_source(&path);
                let product = display_label(application, platform);
                let duration = format!("{:.3}", duration_ms as f64 / 1000.0);
                let kibana_link = kibana_link.unwrap_or_default();
                send_event(
                    &tx,
                    replace_job_event(
                        job_id,
                        template::JobCompleted {
                            job_id,
                            status_class: if execution_error.is_some() {
                                "status-error"
                            } else {
                                completed_status_class(&outcome)
                            },
                            heading: if execution_error.is_some() {
                                "⚠️ Diagnostic completed with output failures"
                            } else {
                                completed_heading(&outcome)
                            },
                            diagnostic_id: &diagnostic_id,
                            docs_created: &docs_created,
                            duration: &duration,
                            source: &source,
                            kibana_link: &kibana_link,
                            product: &product,
                            outcome: outcome.as_str(),
                            upload_destination: None,
                            execution_error: execution_error.as_deref(),
                            recorded_failures,
                        },
                    )
                    .for_owner(owner.clone()),
                )
                .await;
            }
            IncludedDiagnosticJobEvent::Skipped {
                job_id,
                path,
                outcome,
                application,
                platform,
                reason,
            } => {
                let source = child_source(&path);
                let product = display_label(application, platform);
                let reason = skipped_child_reason(&reason, &outcome);
                send_event(
                    &tx,
                    replace_job_event(
                        job_id,
                        template::JobSkipped {
                            job_id,
                            source: &source,
                            product: &product,
                            reason: &reason,
                        },
                    )
                    .for_owner(owner.clone()),
                )
                .await;
            }
            IncludedDiagnosticJobEvent::Failed { job_id, path, error } => {
                let source = child_source(&path);
                send_event(
                    &tx,
                    replace_job_event(
                        job_id,
                        template::JobFailed {
                            job_id,
                            error: &error,
                            source: &source,
                        },
                    )
                    .for_owner(owner.clone()),
                )
                .await;
            }
        }
    }
}

fn child_source(path: &str) -> String {
    format!("Included diagnostic: {path}")
}

fn recorded_report_failures(report: &crate::processor::DiagnosticReport) -> Vec<String> {
    report
        .events()
        .iter()
        .filter(|event| event.severity != EventSeverity::Success)
        .map(|event| format!("{:?} {}: {}", event.severity, event.source, event.reason))
        .collect()
}

fn completed_status_class(outcome: &DiagnosticOutcome) -> &'static str {
    match outcome {
        DiagnosticOutcome::Failed => "status-error",
        DiagnosticOutcome::Partial => "status-info",
        DiagnosticOutcome::Complete | DiagnosticOutcome::Skipped(_) => "status-success",
    }
}

fn completed_heading(outcome: &DiagnosticOutcome) -> &'static str {
    match outcome {
        DiagnosticOutcome::Failed => "❌ Processing failed",
        DiagnosticOutcome::Partial => "⚠️ Processing partially complete",
        DiagnosticOutcome::Complete | DiagnosticOutcome::Skipped(_) => "✅ Processing complete!",
    }
}

fn skipped_child_reason(reason: &str, outcome: &DiagnosticOutcome) -> String {
    match outcome.skip_kind() {
        Some(SkipKind::ByDesign) => format!("{reason} (by design)"),
        Some(SkipKind::NotImplemented) => format!("{reason} (not implemented)"),
        None => reason.to_string(),
    }
}

fn explicit_process_selection(signals: &JobRunSignals) -> Result<Option<ProcessSelection>> {
    let has_explicit_choice = !signals.job.process.selected.trim().is_empty()
        || signals.job.process.product != "elasticsearch"
        || signals.job.process.diagnostic_type != "standard";
    if !has_explicit_choice {
        return Ok(None);
    }

    let selected: Vec<String> = signals
        .job
        .process
        .selected
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect();
    let selected = ApiResolver::resolve_processing_selection(
        &signals.job.process.product,
        &signals.job.process.diagnostic_type,
        &selected,
    )?;

    Ok(Some(ProcessSelection {
        product: signals.job.process.product.clone(),
        diagnostic_type: signals.job.process.diagnostic_type.clone(),
        selected,
    }))
}

fn processing_job_event(_replace_existing_entry: bool, _job_id: u64, template: impl askama::Template) -> ServerEvent {
    job_feed_event(template)
}

fn terminal_job_event(replace_existing_entry: bool, job_id: u64, template: impl askama::Template) -> ServerEvent {
    if replace_existing_entry {
        replace_job_event(job_id, template)
    } else {
        template_event(template)
    }
}

async fn select_processed_exporter(state: Arc<ServerState>, signals: &JobRunSignals) -> Result<Exporter> {
    match signals.job.send.mode {
        SendMode::Remote => {
            let Some(target) = signals
                .job
                .send
                .remote_target
                .as_deref()
                .map(str::trim)
                .filter(|target| !target.is_empty())
            else {
                return Exporter::try_from(Uri::try_from_output_env()?);
            };

            let configured = state.exporter.read().await.clone();
            if target == configured.target_uri() {
                Ok(configured)
            } else {
                let uri = Uri::try_from(target.to_string())?;
                validate_remote_send_uri(&uri)?;
                Exporter::try_from(uri)
            }
        }
        SendMode::Local => {
            let target = signals.job.send.local_target.trim();
            if target == "directory" {
                if !state.server_policy.allows_local_runtime_features() {
                    return Err(eyre!("Service mode does not allow local directory output"));
                }
                let directory = signals.job.send.local_directory.trim();
                if directory.is_empty() {
                    return Err(eyre!("Local directory output requires a directory path"));
                }
                Exporter::try_from(Uri::try_from(directory.to_string())?)
            } else if target.is_empty() {
                Err(eyre!("Local send requires a localhost host or local directory"))
            } else {
                let uri = Uri::try_from(target.to_string())?;
                validate_local_send_uri(&uri)?;
                Exporter::try_from(uri)
            }
        }
    }
}

async fn validate_job_request(state: &ServerState, signals: &JobRunSignals, job: &JobRequest) -> Result<()> {
    if signals.job.collect.source == CollectSource::KnownHost && !state.server_policy.allows_host_management() {
        return Err(eyre!(
            "Service mode requires explicit endpoint and API key instead of saved known hosts"
        ));
    }

    if signals.job.send.mode == SendMode::Local {
        if signals.job.process.mode == ProcessMode::Forward {
            if matches!(
                job.input,
                JobInput::FromRemoteHost { .. } | JobInput::FromServiceLink { .. }
            ) && !signals.job.collect.save
            {
                return Err(eyre!(
                    "Forward + Local requires Download Archive in Collect so the bundle can be retained for browser download"
                ));
            }

            if matches!(
                job.input,
                JobInput::LocalArchive {
                    cleanup_path: Some(_),
                    ..
                }
            ) && !signals.job.collect.save
            {
                return Err(eyre!(
                    "Forward + Local for uploaded archives requires a save-capable collect source"
                ));
            }
        }

        let target = signals.job.send.local_target.trim();
        if signals.job.process.mode == ProcessMode::Process
            && target == "directory"
            && !state.server_policy.allows_local_runtime_features()
        {
            return Err(eyre!("Service mode does not allow local directory output"));
        }
    }

    Ok(())
}

fn validate_local_send_uri(uri: &Uri) -> Result<()> {
    match uri {
        Uri::KnownHost(host) => {
            if !host.has_role(HostRole::Send) {
                return Err(eyre!("Local known-host send targets must have the `send` role"));
            }
            let url = host.get_url()?;
            let host_name = url
                .host_str()
                .ok_or_else(|| eyre!("Local send host is missing a hostname"))?;
            if !matches!(host_name, "localhost" | "127.0.0.1") {
                return Err(eyre!(
                    "Local known-host send targets must resolve to localhost or 127.0.0.1"
                ));
            }
            Ok(())
        }
        _ => Err(eyre!(
            "Local processed send must target a localhost known host or a local directory"
        )),
    }
}

fn validate_remote_send_uri(uri: &Uri) -> Result<()> {
    if let Uri::KnownHost(host) = uri {
        if !host.has_role(HostRole::Send) {
            return Err(eyre!("Remote known-host send targets must have the `send` role"));
        }
        if host.app() != Some(Application::Elasticsearch) {
            return Err(eyre!("Remote known-host send targets must be Elasticsearch hosts"));
        }
    }
    Ok(())
}

#[cfg(test)]
async fn download_service_link_to_path(uri: &Uri, path: &Path) -> Result<()> {
    let Uri::ServiceLink(url) = uri else {
        return Err(eyre!("Expected an authenticated Elastic Upload Service URL"));
    };

    let mut download_url = url.clone();
    let token = download_url
        .password()
        .ok_or_else(|| eyre!("Elastic Upload Service token is missing"))?
        .to_string();
    download_url.set_username("").ok();
    download_url.set_password(None).ok();

    let client = reqwest::Client::new();
    let response = client.get(download_url).header("Authorization", token).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(eyre!("Elastic Upload Service download failed with HTTP {}", status));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let mut file = File::create(path).await?;
    let mut wrote_bytes = false;
    let mut response = response;
    while let Some(chunk) = response.chunk().await? {
        if !chunk.is_empty() {
            wrote_bytes = true;
            file.write_all(&chunk).await?;
        }
    }
    file.flush().await?;
    if !wrote_bytes {
        return Err(eyre!("Downloaded empty file, check upload link expiration"));
    }
    Ok(())
}

async fn publish_retained_download(
    state: &Arc<ServerState>,
    request_user: &str,
    download_token: &str,
    filename: String,
    path: PathBuf,
    cleanup_path: Option<PathBuf>,
) -> Result<()> {
    let token = state
        .insert_retained_bundle_with_token(
            Some(download_token),
            request_user.to_string(),
            filename.clone(),
            path,
            cleanup_path,
            RETAINED_BUNDLE_TTL,
        )
        .await;
    state.schedule_retained_bundle_cleanup(token.clone(), RETAINED_BUNDLE_TTL);
    Ok(())
}

fn merged_identifiers(
    mut base: Identifiers,
    overrides: Identifiers,
    request_user: String,
    input: &JobInput,
) -> Identifiers {
    if overrides.account.is_some() {
        base.account = overrides.account;
    }
    if overrides.case_number.is_some() {
        base.case_number = overrides.case_number;
    }
    if overrides.opportunity.is_some() {
        base.opportunity = overrides.opportunity;
    }
    if overrides.parent_id.is_some() {
        base.parent_id = overrides.parent_id;
    }
    if overrides.platform.is_some() {
        base.platform = overrides.platform;
    }

    base.user = Some(request_user);
    base.filename = overrides.filename.or_else(|| match input {
        JobInput::LocalArchive { filename, .. } => Some(filename.clone()),
        _ => base.filename.clone(),
    });
    base
}

async fn send_event(tx: &mpsc::Sender<ServerEvent>, event: ServerEvent) {
    let _ = tx.send(event).await;
}

async fn send_terminal_signal(tx: &mpsc::Sender<ServerEvent>, state: &ServerState) {
    send_event(
        tx,
        signal_event(format!(
            r#"{{"loading":false,"processing":false,"archive":{{"download_token":""}},"stats":{}}}"#,
            state.get_stats().await
        )),
    )
    .await;
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::{
        completed_heading, completed_status_class, download_service_link_to_path, run_job, select_processed_exporter,
        skipped_child_reason, validate_job_request, validate_local_send_uri, validate_remote_send_uri,
    };
    use crate::{
        data::{Application, CollectMode, HostRole, JobDraft, KnownHostBuilder, Uri},
        exporter::Exporter,
        processor::{DiagnosticOutcome, IncludedDiagnosticJobEvent, SkipKind},
        server::{
            CollectSource, JobInput, JobRequest, JobRunSignals, ProcessMode, RetainedBundle, RuntimeMode, SendMode,
            ServerEvent, ServerPolicy, ServerState, Stats,
        },
    };
    use axum::{Router, http::StatusCode, routing::get};
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };
    use tokio::net::TcpListener;
    use tokio::sync::{RwLock, broadcast, mpsc, watch};
    use url::Url;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_env_lock()
    }

    fn test_state(mode: RuntimeMode) -> ServerState {
        let (stats_updates_tx, stats_updates_rx) = watch::channel(0u64);
        ServerState {
            exporter: Arc::new(RwLock::new(Exporter::default())),
            kibana_url: Arc::new(RwLock::new(String::new())),
            job_requests: Arc::new(RwLock::new(HashMap::new())),
            retained_bundles: Arc::new(RwLock::new(HashMap::<String, RetainedBundle>::new())),
            runtime_mode: mode,
            server_policy: ServerPolicy::defaults(mode),
            #[cfg(feature = "keystore")]
            keystore_rate_limit: Arc::new(std::sync::Mutex::new(
                crate::server::keystore::KeystoreRateLimit::default(),
            )),
            stats: Arc::new(RwLock::new(Stats::default())),
            active_jobs_by_owner: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            shutdown: watch::channel(false).1,
            event_tx: broadcast::channel::<ServerEvent>(8).0,
            stats_updates_tx,
            stats_updates_rx,
        }
    }

    #[test]
    fn validate_local_send_uri_accepts_localhost_send_host() {
        let host = KnownHostBuilder::new(Url::parse("http://localhost:9200").unwrap())
            .roles(vec![HostRole::Send])
            .build()
            .unwrap();
        let uri = Uri::try_from(host).unwrap();
        assert!(validate_local_send_uri(&uri).is_ok());
    }

    #[test]
    fn validate_local_send_uri_rejects_non_local_host() {
        let host = KnownHostBuilder::new(Url::parse("http://example.com:9200").unwrap())
            .roles(vec![HostRole::Send])
            .build()
            .unwrap();
        let uri = Uri::try_from(host).unwrap();
        assert!(validate_local_send_uri(&uri).is_err());
    }

    #[test]
    fn web_draft_preserves_raw_send_alongside_processed_output() {
        let mut draft = JobDraft::default();
        draft.process.mode = ProcessMode::Process;
        draft.process.enabled = true;
        draft.send.mode = SendMode::Remote;
        draft.send.remote_target = Some("processed-cluster".to_string());
        draft.send.raw_remote_target = Some("raw-upload".to_string());

        assert_eq!(
            super::raw_send_target(&draft).map(|target| target.upload_id),
            Some("raw-upload".to_string())
        );
    }

    #[test]
    fn forward_web_draft_uses_legacy_remote_target_as_raw_send() {
        let mut draft = JobDraft::default();
        draft.process.mode = ProcessMode::Forward;
        draft.process.enabled = false;
        draft.send.mode = SendMode::Remote;
        draft.send.remote_target = Some("raw-upload".to_string());

        assert_eq!(
            super::raw_send_target(&draft).map(|target| target.upload_id),
            Some("raw-upload".to_string())
        );
    }

    #[tokio::test]
    async fn service_mode_allows_bundle_save_downloads() {
        let state = test_state(RuntimeMode::Service);
        let mut signals = JobRunSignals::default();
        signals.job.collect.source = CollectSource::ApiKey;
        signals.job.collect.save = true;

        let job = JobRequest {
            owner: "test@example.com".to_string(),
            identifiers: Default::default(),
            input: JobInput::LocalArchive {
                source: "upload.zip".to_string(),
                filename: "upload.zip".to_string(),
                path: "/tmp/upload.zip".into(),
                cleanup_path: None,
            },
        };

        assert!(validate_job_request(&state, &signals, &job).await.is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn loaded_bundle_process_retention_publishes_owner_scoped_download_without_save() {
        let output = tempfile::tempdir().expect("processed output");
        let archive = std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/archives/elasticsearch-api-diagnostics-9.3.3.zip"
        ));
        let token = "loaded-bundle-token";
        let owner = "owner@example.com";
        let state = Arc::new(test_state(RuntimeMode::User));
        let mut signals = JobRunSignals::default();
        signals.job.collect.mode = CollectMode::Upload;
        signals.job.collect.source = CollectSource::UploadFile;
        signals.job.process.enabled = true;
        signals.job.send.mode = SendMode::Local;
        signals.job.send.local_target = "directory".to_string();
        signals.job.send.local_directory = output.path().display().to_string();
        signals.job.send.raw_local = true;
        signals.archive.download_token = token.to_string();
        let request = JobRequest {
            owner: owner.to_string(),
            identifiers: Default::default(),
            input: JobInput::LocalArchive {
                source: archive.display().to_string(),
                filename: "uploaded.zip".to_string(),
                path: archive,
                cleanup_path: Some(std::path::PathBuf::from("/tmp/unused-cleanup-path")),
            },
        };
        let (tx, mut rx) = mpsc::channel(32);

        run_job(state.clone(), signals, 88, owner.to_string(), tx, request, false).await;

        let retained = state.retained_bundle(token).await.expect("retained download");
        assert_eq!(retained.owner, owner);
        assert!(retained.path.as_ref().expect("retained path").exists());
        assert!(
            retained
                .path
                .as_ref()
                .expect("retained path")
                .parent()
                .expect("retained directory")
                .to_string_lossy()
                .contains("esdiag-retained-88"),
            "a loaded bundle retains through execution policy, not Save"
        );
        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(
            events.iter().any(|event| event.owner() == owner),
            "completion and retention signals remain owner-scoped"
        );
    }

    #[tokio::test]
    async fn service_link_save_does_not_require_directory() {
        let state = test_state(RuntimeMode::User);
        let mut signals = JobRunSignals::default();
        signals.job.collect.save = true;

        let job = JobRequest {
            owner: "test@example.com".to_string(),
            identifiers: Default::default(),
            input: JobInput::FromServiceLink {
                source: "downloaded.zip".to_string(),
                uri: Uri::ServiceLink(Url::parse("https://token:secret@example.com/archive.zip").unwrap()),
            },
        };

        assert!(validate_job_request(&state, &signals, &job).await.is_ok());
    }

    #[tokio::test]
    async fn forward_local_temp_upload_requires_save_server_side() {
        let state = test_state(RuntimeMode::User);
        let mut signals = JobRunSignals::default();
        signals.job.process.mode = ProcessMode::Forward;
        signals.job.send.mode = SendMode::Local;

        let job = JobRequest {
            owner: "test@example.com".to_string(),
            identifiers: Default::default(),
            input: JobInput::LocalArchive {
                source: "upload.zip".to_string(),
                filename: "upload.zip".to_string(),
                path: "/tmp/upload.zip".into(),
                cleanup_path: Some("/tmp/upload.zip".into()),
            },
        };

        assert!(validate_job_request(&state, &signals, &job).await.is_err());
    }

    #[tokio::test]
    async fn child_diagnostic_events_inherit_parent_owner() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let (child_tx, child_rx) = tokio::sync::mpsc::unbounded_channel();
        let owner = "alice@example.com".to_string();

        let handle = tokio::spawn(super::render_child_diagnostic_events(tx, owner.clone(), child_rx));
        child_tx
            .send(IncludedDiagnosticJobEvent::Queued {
                job_id: 7,
                path: "elasticsearch".to_string(),
            })
            .expect("send child event");
        drop(child_tx);

        let event = rx.recv().await.expect("child job event");
        assert_eq!(event.owner(), owner);
        handle.await.expect("child renderer should complete");
    }

    #[tokio::test]
    async fn child_outcome_projection_inserts_before_terminal_replace() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let mut outcome = crate::job::outcome::ExecutionOutcome::new(crate::job::context::ExecutionIdentity::new(
            1,
            "alice@example.com",
        ));
        let mut child_execution = crate::job::outcome::ExecutionOutcome::new(
            crate::job::context::ExecutionIdentity::new(7, "alice@example.com"),
        );
        child_execution.record(
            crate::job::outcome::Stage::Process,
            crate::job::outcome::StageStatus::Failed("child failed".to_string()),
        );
        outcome.children.push(crate::job::outcome::ChildExecutionOutcome {
            path: "elasticsearch".to_string(),
            execution: Box::new(child_execution),
            diagnostic_outcome: DiagnosticOutcome::Failed,
            application: Some(Application::Elasticsearch),
            platform: crate::data::Platform::ECK,
            runtime: None,
        });

        super::render_child_outcomes(&tx, "alice@example.com", &outcome).await;

        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(matches!(
            events.first(),
            Some(ServerEvent::JobFeed { html, .. }) if html.contains("id=\"job-7\"")
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            ServerEvent::ReplaceSelector { selector, html, .. }
                if selector == "#job-7" && html.contains("Processing Failed")
        )));
    }

    #[tokio::test]
    async fn remote_send_reuses_configured_exporter_for_canonical_target_uri() {
        let state = test_state(RuntimeMode::User);
        let host = KnownHostBuilder::new(Url::parse("https://example.com:9200").unwrap())
            .roles(vec![HostRole::Send])
            .build()
            .unwrap();
        let configured = Exporter::try_from(Uri::try_from(host).unwrap()).expect("configured exporter");
        *state.exporter.write().await = configured.clone();

        let mut signals = JobRunSignals::default();
        signals.job.send.mode = SendMode::Remote;
        signals.job.send.remote_target = Some(configured.target_uri());

        let selected = select_processed_exporter(Arc::new(state), &signals)
            .await
            .expect("select exporter");
        assert_eq!(selected.target_uri(), configured.target_uri());
        assert_eq!(selected.to_string(), configured.to_string());
    }

    #[tokio::test]
    async fn remote_send_without_ui_target_uses_output_environment() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::set_var("ESDIAG_OUTPUT_URL", "http://localhost:9200");
            std::env::set_var("ESDIAG_OUTPUT_APIKEY", "runtime-secret");
            std::env::remove_var("ESDIAG_OUTPUT_USERNAME");
            std::env::remove_var("ESDIAG_OUTPUT_PASSWORD");
        }

        let state = Arc::new(test_state(RuntimeMode::User));
        let mut signals = JobRunSignals::default();
        signals.job.send.mode = SendMode::Remote;
        signals.job.send.remote_target = None;

        let selected = select_processed_exporter(state, &signals)
            .await
            .expect("select environment exporter");
        assert_eq!(selected.target_uri(), "http://localhost:9200/");

        unsafe {
            std::env::remove_var("ESDIAG_OUTPUT_URL");
            std::env::remove_var("ESDIAG_OUTPUT_APIKEY");
        }
    }

    #[tokio::test]
    async fn remote_send_without_ui_target_or_environment_fails() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::remove_var("ESDIAG_OUTPUT_URL");
            std::env::remove_var("ESDIAG_OUTPUT_APIKEY");
            std::env::remove_var("ESDIAG_OUTPUT_USERNAME");
            std::env::remove_var("ESDIAG_OUTPUT_PASSWORD");
        }

        let state = Arc::new(test_state(RuntimeMode::User));
        let mut signals = JobRunSignals::default();
        signals.job.send.mode = SendMode::Remote;
        signals.job.send.remote_target = None;

        let err = match select_processed_exporter(state, &signals).await {
            Ok(_) => panic!("missing UI target and environment must fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("ESDIAG_OUTPUT_URL is not defined"));
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn remote_setup_failure_replaces_processing_entry_and_clears_ui_state() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::remove_var("ESDIAG_OUTPUT_URL");
            std::env::remove_var("ESDIAG_OUTPUT_APIKEY");
            std::env::remove_var("ESDIAG_OUTPUT_USERNAME");
            std::env::remove_var("ESDIAG_OUTPUT_PASSWORD");
        }

        let host = KnownHostBuilder::new(Url::parse("http://cluster.example:9200").unwrap())
            .roles(vec![HostRole::Collect])
            .build()
            .unwrap();
        let mut signals = JobRunSignals::default();
        signals.job.collect.source = CollectSource::ApiKey;
        signals.job.send.mode = SendMode::Remote;
        signals.job.send.remote_target = None;
        let job = JobRequest {
            owner: "Anonymous".to_string(),
            identifiers: Default::default(),
            input: JobInput::FromRemoteHost {
                source: "http://cluster.example:9200".to_string(),
                host,
                diagnostic_type: "standard".to_string(),
            },
        };
        let (tx, mut rx) = mpsc::channel(8);

        run_job(
            Arc::new(test_state(RuntimeMode::User)),
            signals,
            42,
            "Anonymous".to_string(),
            tx,
            job,
            false,
        )
        .await;

        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(matches!(
            events.first(),
            Some(ServerEvent::JobFeed { html, .. })
                if html.contains("id=\"job-42\"") && html.contains("Processing")
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            ServerEvent::ReplaceSelector { selector, html, .. }
                if selector == "#job-42"
                    && html.contains("id=\"job-42\"")
                    && html.contains("Processing Failed")
                    && html.contains("ESDIAG_OUTPUT_URL is not defined")
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            ServerEvent::Signals { payload, .. }
                if payload.contains(r#""loading":false"#)
                    && payload.contains(r#""processing":false"#)
        )));
    }

    #[tokio::test]
    async fn remote_send_validation_rejects_collect_only_known_host() {
        let host = KnownHostBuilder::new(Url::parse("https://example.com:9200").unwrap())
            .application(Application::Elasticsearch)
            .roles(vec![HostRole::Collect])
            .build()
            .unwrap();

        let uri = Uri::try_from(host).expect("known-host uri");
        assert!(validate_remote_send_uri(&uri).is_err());
    }

    #[test]
    fn skipped_child_reason_includes_skip_kind() {
        let reason = skipped_child_reason(
            "Kibana processing is not yet implemented",
            &DiagnosticOutcome::Skipped(SkipKind::NotImplemented),
        );

        assert_eq!(reason, "Kibana processing is not yet implemented (not implemented)");
    }

    #[test]
    fn completed_status_metadata_tracks_outcome() {
        assert_eq!(completed_status_class(&DiagnosticOutcome::Complete), "status-success");
        assert_eq!(
            completed_heading(&DiagnosticOutcome::Complete),
            "✅ Processing complete!"
        );
        assert_eq!(completed_status_class(&DiagnosticOutcome::Partial), "status-info");
        assert_eq!(
            completed_heading(&DiagnosticOutcome::Partial),
            "⚠️ Processing partially complete"
        );
        assert_eq!(completed_status_class(&DiagnosticOutcome::Failed), "status-error");
        assert_eq!(completed_heading(&DiagnosticOutcome::Failed), "❌ Processing failed");
    }

    #[tokio::test]
    async fn service_link_download_surfaces_http_status_before_writing_file() {
        async fn unauthorized() -> StatusCode {
            StatusCode::UNAUTHORIZED
        }

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");
        tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/archive.zip", get(unauthorized)))
                .await
                .expect("serve mock upload endpoint");
        });

        let uri = Uri::ServiceLink(Url::parse(&format!("http://token:secret@{addr}/archive.zip")).expect("mock url"));
        let path = std::env::temp_dir().join("esdiag-service-link-status-test.zip");
        let _ = std::fs::remove_file(&path);
        let err = download_service_link_to_path(&uri, &path)
            .await
            .expect_err("non-success download should fail");

        assert!(
            err.to_string().contains("HTTP 401 Unauthorized"),
            "expected status-bearing error, got: {err}"
        );
        assert!(!path.exists(), "failed download should not create output file");
    }
}
