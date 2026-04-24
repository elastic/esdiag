// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    CollectSource, ProcessMode, SendMode, ServerEvent, ServerState, WorkflowInput, WorkflowJob, WorkflowRunSignals,
    job_feed_event, replace_job_event, signal_event, template, template_event,
};
use crate::{
    data::{HostRole, Product, Uri},
    exporter::Exporter,
    processor::{
        Collector, Identifiers, Processor,
        api::{ApiResolver, ProcessSelection},
        new_job_id,
    },
    receiver::Receiver,
    uploader,
};
use eyre::{Result, eyre};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{fs, fs::File, io::AsyncWriteExt, sync::mpsc};

const RETAINED_BUNDLE_TTL: Duration = Duration::from_secs(3600);

struct JobDescriptor<'a> {
    id: u64,
    source: &'a str,
}

struct WorkflowExecutionContext<'a> {
    state: Arc<ServerState>,
    signals: &'a WorkflowRunSignals,
    job_id: u64,
    source: &'a str,
    identifiers: Identifiers,
    request_user: &'a str,
    tx: &'a mpsc::Sender<ServerEvent>,
    replace_existing_entry: bool,
}

struct LocalArchiveJobContext<'a> {
    state: Arc<ServerState>,
    signals: &'a WorkflowRunSignals,
    job: JobDescriptor<'a>,
    path: PathBuf,
    identifiers: Identifiers,
    tx: &'a mpsc::Sender<ServerEvent>,
    replace_existing_entry: bool,
}

struct ProcessorJobContext<'a> {
    state: Arc<ServerState>,
    tx: &'a mpsc::Sender<ServerEvent>,
    receiver: Arc<Receiver>,
    exporter: Arc<Exporter>,
    identifiers: Identifiers,
    process_selection: Option<ProcessSelection>,
    job: JobDescriptor<'a>,
    replace_existing_entry: bool,
}

pub async fn run_job(
    state: Arc<ServerState>,
    signals: WorkflowRunSignals,
    job_id: u64,
    request_user: String,
    tx: mpsc::Sender<ServerEvent>,
    job: WorkflowJob,
    replace_existing_entry: bool,
) {
    let source = job.source().to_string();
    let download_token = signals.archive.download_token.trim().to_string();
    let should_track_download = signals.workflow.collect.save && !download_token.is_empty();
    let validation = validate_workflow_request(&state, &signals, &job).await;
    if let Err(error) = validation {
        if should_track_download {
            state
                .reject_retained_bundle(&download_token, &request_user, error.to_string(), RETAINED_BUNDLE_TTL)
                .await;
        }
        state.record_failure().await;
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

    let result = match &job.input {
        WorkflowInput::LocalArchive { path, .. } => {
            execute_local_archive_job(LocalArchiveJobContext {
                state: state.clone(),
                signals: &signals,
                job: JobDescriptor {
                    id: job_id,
                    source: &source,
                },
                path: path.clone(),
                identifiers,
                tx: &tx,
                replace_existing_entry,
            })
            .await
        }
        WorkflowInput::FromServiceLink { uri, .. } => {
            execute_service_link_job(
                WorkflowExecutionContext {
                    state: state.clone(),
                    signals: &signals,
                    job_id,
                    source: &source,
                    identifiers,
                    request_user: &request_user,
                    tx: &tx,
                    replace_existing_entry,
                },
                uri.clone(),
            )
            .await
        }
        WorkflowInput::FromRemoteHost {
            host, diagnostic_type, ..
        } => {
            execute_remote_collection_job(
                WorkflowExecutionContext {
                    state: state.clone(),
                    signals: &signals,
                    job_id,
                    source: &source,
                    identifiers,
                    request_user: &request_user,
                    tx: &tx,
                    replace_existing_entry,
                },
                host.clone(),
                diagnostic_type.clone(),
            )
            .await
        }
    };

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
        state.record_failure().await;
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
    }

    send_terminal_signal(&tx, &state).await;
    job.cleanup().await;
}

async fn execute_local_archive_job(ctx: LocalArchiveJobContext<'_>) -> Result<()> {
    let LocalArchiveJobContext {
        state,
        signals,
        job,
        path,
        identifiers,
        tx,
        replace_existing_entry,
    } = ctx;

    match signals.workflow.process.mode {
        ProcessMode::Process => {
            let receiver = Arc::new(Receiver::try_from(Uri::File(path))?);
            let exporter = Arc::new(select_processed_exporter(state.clone(), signals).await?);
            let process_selection = explicit_process_selection(signals)?;
            run_processor_job(ProcessorJobContext {
                state,
                tx,
                receiver,
                exporter,
                identifiers,
                process_selection,
                job,
                replace_existing_entry,
            })
            .await
        }
        ProcessMode::Forward => {
            run_forward_job(state, tx, signals, job.id, job.source, &path, replace_existing_entry).await
        }
    }
}

async fn execute_service_link_job(ctx: WorkflowExecutionContext<'_>, uri: Uri) -> Result<()> {
    let WorkflowExecutionContext {
        state,
        signals,
        job_id,
        source,
        identifiers,
        request_user,
        tx,
        replace_existing_entry,
    } = ctx;

    if signals.workflow.collect.save {
        state.record_job_started().await;
        if !replace_existing_entry {
            send_event(tx, job_feed_event(template::JobCollectionProcessing { job_id, source })).await;
        }
        send_event(tx, signal_event(r#"{"loading":false,"processing":true}"#)).await;

        let collected = collect_service_link_archive(job_id, uri, source, signals, identifiers).await?;
        if let WorkflowInput::LocalArchive { path, .. } = collected.input {
            let archive_filename = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("diagnostic.zip")
                .to_string();
            publish_retained_download(
                &state,
                request_user,
                &signals.archive.download_token,
                archive_filename.clone(),
                path.clone(),
                None,
            )
            .await?;
            state.record_success(0, 0).await;
            send_event(
                tx,
                replace_job_event(
                    job_id,
                    template::JobCollectionCompleted {
                        job_id,
                        source,
                        archive_path: &archive_filename,
                    },
                ),
            )
            .await;
            let handoff_job_id = new_job_id();
            return execute_local_archive_job(LocalArchiveJobContext {
                state,
                signals,
                job: JobDescriptor {
                    id: handoff_job_id,
                    source,
                },
                path,
                identifiers: collected.identifiers,
                tx,
                replace_existing_entry: false,
            })
            .await;
        }

        return Err(eyre!("Service link collection did not produce a local archive"));
    }

    match signals.workflow.process.mode {
        ProcessMode::Process => {
            let receiver = Arc::new(Receiver::try_from(uri)?);
            let exporter = Arc::new(select_processed_exporter(state.clone(), signals).await?);
            let process_selection = explicit_process_selection(signals)?;
            run_processor_job(ProcessorJobContext {
                state,
                tx,
                receiver,
                exporter,
                identifiers,
                process_selection,
                job: JobDescriptor { id: job_id, source },
                replace_existing_entry: false,
            })
            .await
        }
        ProcessMode::Forward => {
            let path = download_service_link_to_temp(&uri, job_id, source).await?;
            let result = run_forward_job(state, tx, signals, job_id, source, &path, false).await;
            cleanup_local_path(&path).await;
            result
        }
    }
}

async fn execute_remote_collection_job(
    ctx: WorkflowExecutionContext<'_>,
    host: crate::data::KnownHost,
    diagnostic_type: String,
) -> Result<()> {
    let WorkflowExecutionContext {
        state,
        signals,
        job_id,
        identifiers,
        request_user,
        tx,
        replace_existing_entry,
        ..
    } = ctx;

    let source = host.get_url().to_string();
    if signals.workflow.process.mode == ProcessMode::Process && !signals.workflow.collect.save {
        let receiver = Arc::new(Receiver::try_from(host)?);
        let exporter = Arc::new(select_processed_exporter(state.clone(), signals).await?);
        let process_selection = explicit_process_selection(signals)?;
        return run_processor_job(ProcessorJobContext {
            state,
            tx,
            receiver,
            exporter,
            identifiers,
            process_selection,
            job: JobDescriptor {
                id: job_id,
                source: &source,
            },
            replace_existing_entry,
        })
        .await;
    }

    if signals.workflow.collect.save {
        state.record_job_started().await;
        if !replace_existing_entry {
            send_event(
                tx,
                job_feed_event(template::JobCollectionProcessing {
                    job_id,
                    source: &source,
                }),
            )
            .await;
        }
        send_event(tx, signal_event(r#"{"loading":false,"processing":true}"#)).await;
    }

    let collected = collect_remote_archive(job_id, host, &diagnostic_type, signals, identifiers).await?;
    let cleanup_path = match &collected.input {
        WorkflowInput::LocalArchive {
            cleanup_path: Some(path),
            ..
        } => Some(path.clone()),
        _ => None,
    };

    let result = if let WorkflowInput::LocalArchive { path, cleanup_path, .. } = collected.input {
        if signals.workflow.collect.save {
            let archive_filename = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("diagnostic.zip")
                .to_string();
            publish_retained_download(
                &state,
                request_user,
                &signals.archive.download_token,
                archive_filename.clone(),
                path.clone(),
                cleanup_path,
            )
            .await?;
            state.record_success(0, 0).await;
            send_event(
                tx,
                replace_job_event(
                    job_id,
                    template::JobCollectionCompleted {
                        job_id,
                        source: &source,
                        archive_path: &archive_filename,
                    },
                ),
            )
            .await;
            let handoff_job_id = new_job_id();
            execute_local_archive_job(LocalArchiveJobContext {
                state,
                signals,
                job: JobDescriptor {
                    id: handoff_job_id,
                    source: &source,
                },
                path,
                identifiers: collected.identifiers,
                tx,
                replace_existing_entry: false,
            })
            .await
        } else {
            execute_local_archive_job(LocalArchiveJobContext {
                state,
                signals,
                job: JobDescriptor {
                    id: job_id,
                    source: &source,
                },
                path,
                identifiers: collected.identifiers,
                tx,
                replace_existing_entry,
            })
            .await
        }
    } else {
        Err(eyre!("Remote collection did not produce a local archive"))
    };

    if let Some(path) = cleanup_path {
        cleanup_local_path(&path).await;
    }

    result
}

async fn run_processor_job(ctx: ProcessorJobContext<'_>) -> Result<()> {
    let ProcessorJobContext {
        state,
        tx,
        receiver,
        exporter,
        identifiers,
        process_selection,
        job,
        replace_existing_entry,
    } = ctx;

    let processor = Processor::try_new_with_selection(receiver, exporter, identifiers, process_selection).await?;
    let processor = processor.start().await.map_err(|failed| eyre!(failed.state.error))?;
    state.record_job_started().await;

    if !replace_existing_entry {
        send_event(
            tx,
            processing_job_event(
                replace_existing_entry,
                job.id,
                template::JobProcessing {
                    job_id: job.id,
                    source: job.source,
                },
            ),
        )
        .await;
    }
    send_event(tx, signal_event(r#"{"loading":false,"processing":true}"#)).await;

    match processor.process().await {
        Ok(completed) => {
            let report = &completed.state.report;
            state
                .record_success(report.diagnostic.docs.total, report.diagnostic.docs.errors)
                .await;
            send_event(
                tx,
                terminal_job_event(
                    replace_existing_entry,
                    job.id,
                    template::JobCompleted {
                        job_id: job.id,
                        diagnostic_id: &report.diagnostic.metadata.id,
                        docs_created: &report.diagnostic.docs.created,
                        duration: &format!("{:.3}", report.diagnostic.processing_duration as f64 / 1000.0),
                        source: job.source,
                        kibana_link: report.diagnostic.kibana_link.as_ref().unwrap_or(&"#".to_string()),
                        product: &report.diagnostic.product.to_string(),
                    },
                ),
            )
            .await;
            Ok(())
        }
        Err(failed) => Err(eyre!(failed.state.error)),
    }
}

fn explicit_process_selection(signals: &WorkflowRunSignals) -> Result<Option<ProcessSelection>> {
    let has_explicit_choice = !signals.workflow.process.selected.trim().is_empty()
        || signals.workflow.process.product != "elasticsearch"
        || signals.workflow.process.diagnostic_type != "standard";
    if !has_explicit_choice {
        return Ok(None);
    }

    let selected: Vec<String> = signals
        .workflow
        .process
        .selected
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect();
    let selected = ApiResolver::resolve_processing_selection(
        &signals.workflow.process.product,
        &signals.workflow.process.diagnostic_type,
        &selected,
    )?;

    Ok(Some(ProcessSelection {
        product: signals.workflow.process.product.clone(),
        diagnostic_type: signals.workflow.process.diagnostic_type.clone(),
        selected,
    }))
}

async fn run_forward_job(
    state: Arc<ServerState>,
    tx: &mpsc::Sender<ServerEvent>,
    signals: &WorkflowRunSignals,
    job_id: u64,
    source: &str,
    path: &Path,
    replace_existing_entry: bool,
) -> Result<()> {
    if signals.workflow.send.mode == SendMode::Local {
        if !replace_existing_entry {
            send_event(
                tx,
                processing_job_event(
                    replace_existing_entry,
                    job_id,
                    template::JobForwardProcessing { job_id, source },
                ),
            )
            .await;
        }
        state.record_job_started().await;
        send_event(tx, signal_event(r#"{"loading":false,"processing":true}"#)).await;

        let destination = Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| format!("Browser download started for {name}"))
            .unwrap_or_else(|| "Browser download started".to_string());
        state.record_success(0, 0).await;
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
        return Ok(());
    }

    let target = signals.workflow.send.remote_target.trim();
    if target.is_empty() {
        return Err(eyre!(
            "Remote forward requires an Elastic Upload Service upload id or URL"
        ));
    }

    if !replace_existing_entry {
        send_event(
            tx,
            processing_job_event(
                replace_existing_entry,
                job_id,
                template::JobForwardProcessing { job_id, source },
            ),
        )
        .await;
    }
    state.record_job_started().await;
    send_event(tx, signal_event(r#"{"loading":false,"processing":true}"#)).await;

    let response = uploader::upload_file(path, target, uploader::DEFAULT_UPLOAD_API_URL).await?;
    state.record_success(0, 0).await;
    let destination = format!("https://upload.elastic.co/g/{}", response.slug);
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
    Ok(())
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

async fn select_processed_exporter(state: Arc<ServerState>, signals: &WorkflowRunSignals) -> Result<Exporter> {
    match signals.workflow.send.mode {
        SendMode::Remote => {
            let configured = state.exporter.read().await.clone();
            let configured_target = configured.target_uri();
            let target = signals.workflow.send.remote_target.trim();
            if target.is_empty() || target == configured_target {
                Ok(configured)
            } else {
                let uri = Uri::try_from(target.to_string())?;
                validate_remote_send_uri(&uri)?;
                Exporter::try_from(uri)
            }
        }
        SendMode::Local => {
            let target = signals.workflow.send.local_target.trim();
            if target == "directory" {
                if !state.runtime_mode_policy.allows_local_runtime_features() {
                    return Err(eyre!("Service mode does not allow local directory output"));
                }
                let directory = signals.workflow.send.local_directory.trim();
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

async fn validate_workflow_request(state: &ServerState, signals: &WorkflowRunSignals, job: &WorkflowJob) -> Result<()> {
    if signals.workflow.collect.source == CollectSource::KnownHost
        && !state.runtime_mode_policy.allows_host_management()
    {
        return Err(eyre!(
            "Service mode requires explicit endpoint and API key instead of saved known hosts"
        ));
    }

    if signals.workflow.send.mode == SendMode::Local {
        if signals.workflow.process.mode == ProcessMode::Forward {
            if matches!(
                job.input,
                WorkflowInput::FromRemoteHost { .. } | WorkflowInput::FromServiceLink { .. }
            ) && !signals.workflow.collect.save
            {
                return Err(eyre!(
                    "Forward + Local requires Download Archive in Collect so the bundle can be retained for browser download"
                ));
            }

            if matches!(
                job.input,
                WorkflowInput::LocalArchive {
                    cleanup_path: Some(_),
                    ..
                }
            ) && !signals.workflow.collect.save
            {
                return Err(eyre!(
                    "Forward + Local for uploaded archives requires a save-capable collect source"
                ));
            }
        }

        let target = signals.workflow.send.local_target.trim();
        if signals.workflow.process.mode == ProcessMode::Process
            && target == "directory"
            && !state.runtime_mode_policy.allows_local_runtime_features()
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
            let url = host.get_url();
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
        if host.app() != &Product::Elasticsearch {
            return Err(eyre!("Remote known-host send targets must be Elasticsearch hosts"));
        }
    }
    Ok(())
}

async fn collect_remote_archive(
    job_id: u64,
    host: crate::data::KnownHost,
    diagnostic_type: &str,
    signals: &WorkflowRunSignals,
    identifiers: Identifiers,
) -> Result<WorkflowJob> {
    let temp_dir = std::env::temp_dir().join(format!("esdiag-workflow-{job_id}"));
    std::fs::create_dir_all(&temp_dir)?;
    let (output_dir, cleanup_path) = if signals.workflow.collect.save {
        (temp_dir.clone(), Some(temp_dir.clone()))
    } else {
        (temp_dir.clone(), Some(temp_dir))
    };

    let source = host.get_url().to_string();
    let receiver = Receiver::try_from(host.clone())?;
    let exporter = Exporter::for_collect_archive(output_dir)?;
    let collector = Collector::try_new(
        receiver,
        exporter,
        host.app().clone(),
        diagnostic_type.to_string(),
        None,
        None,
        identifiers.clone(),
    )
    .await?;
    let result = collector.collect().await?;
    let path = PathBuf::from(result.path.clone());
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| eyre!("Collected archive path is missing a filename"))?
        .to_string();

    Ok(WorkflowJob {
        identifiers: identifiers.with_filename(Some(filename.clone())),
        input: WorkflowInput::LocalArchive {
            source,
            filename,
            path,
            cleanup_path,
        },
    })
}

async fn collect_service_link_archive(
    job_id: u64,
    uri: Uri,
    source: &str,
    signals: &WorkflowRunSignals,
    identifiers: Identifiers,
) -> Result<WorkflowJob> {
    let filename = local_archive_filename(source, job_id)?;
    let path = std::env::temp_dir().join(format!("esdiag-service-link-{job_id}-{filename}"));
    let cleanup_path = if signals.workflow.collect.save {
        None
    } else {
        Some(path.clone())
    };

    download_service_link_to_path(&uri, &path).await?;

    Ok(WorkflowJob {
        identifiers: identifiers.with_filename(Some(filename.clone())),
        input: WorkflowInput::LocalArchive {
            source: source.to_string(),
            filename,
            path,
            cleanup_path,
        },
    })
}

async fn download_service_link_to_temp(uri: &Uri, job_id: u64, source: &str) -> Result<PathBuf> {
    let filename = local_archive_filename(source, job_id)?;
    let temp_path = std::env::temp_dir().join(format!("esdiag-service-link-{job_id}-{filename}"));
    download_service_link_to_path(uri, &temp_path).await?;
    Ok(temp_path)
}

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

fn local_archive_filename(source: &str, job_id: u64) -> Result<String> {
    let filename = Path::new(source)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("service-link-{job_id}.zip"));
    if filename.contains(std::path::MAIN_SEPARATOR) {
        return Err(eyre!("Invalid archive filename"));
    }
    Ok(filename)
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
    input: &WorkflowInput,
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
    if overrides.orchestration.is_some() {
        base.orchestration = overrides.orchestration;
    }

    base.user = Some(request_user);
    base.filename = overrides.filename.or_else(|| match input {
        WorkflowInput::LocalArchive { filename, .. } => Some(filename.clone()),
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

async fn cleanup_local_path(path: &Path) {
    if let Err(err) = fs::remove_file(path).await
        && err.kind() != std::io::ErrorKind::NotFound
    {
        tracing::debug!("Failed to clean local workflow path {}: {}", path.display(), err);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        download_service_link_to_path, select_processed_exporter, validate_local_send_uri, validate_remote_send_uri,
        validate_workflow_request,
    };
    use crate::{
        data::{HostRole, KnownHostBuilder, Product, Uri},
        exporter::Exporter,
        server::{
            CollectSource, ProcessMode, RetainedBundle, RuntimeMode, RuntimeModePolicy, SendMode, ServerEvent,
            ServerState, Stats, WorkflowInput, WorkflowJob, WorkflowRunSignals,
        },
    };
    use axum::{Router, http::StatusCode, routing::get};
    use std::{collections::HashMap, sync::Arc};
    use tokio::net::TcpListener;
    use tokio::sync::{RwLock, broadcast, watch};
    use url::Url;

    fn test_state(mode: RuntimeMode) -> ServerState {
        let (stats_updates_tx, stats_updates_rx) = watch::channel(0u64);
        ServerState {
            exporter: Arc::new(RwLock::new(Exporter::default())),
            kibana_url: Arc::new(RwLock::new(String::new())),
            workflow_jobs: Arc::new(RwLock::new(HashMap::new())),
            retained_bundles: Arc::new(RwLock::new(HashMap::<String, RetainedBundle>::new())),
            runtime_mode: mode,
            runtime_mode_policy: RuntimeModePolicy::new(mode),
            #[cfg(feature = "keystore")]
            keystore_rate_limit: Arc::new(std::sync::Mutex::new(
                crate::server::keystore::KeystoreRateLimit::default(),
            )),
            stats: Arc::new(RwLock::new(Stats::default())),
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

    #[tokio::test]
    async fn service_mode_allows_bundle_save_downloads() {
        let state = test_state(RuntimeMode::Service);
        let mut signals = WorkflowRunSignals::default();
        signals.workflow.collect.source = CollectSource::ApiKey;
        signals.workflow.collect.save = true;

        let job = WorkflowJob {
            identifiers: Default::default(),
            input: WorkflowInput::LocalArchive {
                source: "upload.zip".to_string(),
                filename: "upload.zip".to_string(),
                path: "/tmp/upload.zip".into(),
                cleanup_path: None,
            },
        };

        assert!(validate_workflow_request(&state, &signals, &job).await.is_ok());
    }

    #[tokio::test]
    async fn service_link_save_does_not_require_directory() {
        let state = test_state(RuntimeMode::User);
        let mut signals = WorkflowRunSignals::default();
        signals.workflow.collect.save = true;

        let job = WorkflowJob {
            identifiers: Default::default(),
            input: WorkflowInput::FromServiceLink {
                source: "downloaded.zip".to_string(),
                uri: Uri::ServiceLink(Url::parse("https://token:secret@example.com/archive.zip").unwrap()),
            },
        };

        assert!(validate_workflow_request(&state, &signals, &job).await.is_ok());
    }

    #[tokio::test]
    async fn forward_local_temp_upload_requires_save_server_side() {
        let state = test_state(RuntimeMode::User);
        let mut signals = WorkflowRunSignals::default();
        signals.workflow.process.mode = ProcessMode::Forward;
        signals.workflow.send.mode = SendMode::Local;

        let job = WorkflowJob {
            identifiers: Default::default(),
            input: WorkflowInput::LocalArchive {
                source: "upload.zip".to_string(),
                filename: "upload.zip".to_string(),
                path: "/tmp/upload.zip".into(),
                cleanup_path: Some("/tmp/upload.zip".into()),
            },
        };

        assert!(validate_workflow_request(&state, &signals, &job).await.is_err());
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

        let mut signals = WorkflowRunSignals::default();
        signals.workflow.send.mode = SendMode::Remote;
        signals.workflow.send.remote_target = configured.target_uri();

        let selected = select_processed_exporter(Arc::new(state), &signals)
            .await
            .expect("select exporter");
        assert_eq!(selected.target_uri(), configured.target_uri());
        assert_eq!(selected.to_string(), configured.to_string());
    }

    #[tokio::test]
    async fn remote_send_validation_rejects_collect_only_known_host() {
        let host = KnownHostBuilder::new(Url::parse("https://example.com:9200").unwrap())
            .product(Product::Elasticsearch)
            .roles(vec![HostRole::Collect])
            .build()
            .unwrap();

        let uri = Uri::try_from(host).expect("known-host uri");
        assert!(validate_remote_send_uri(&uri).is_err());
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
