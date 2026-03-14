// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    CollectSource, CollectedArtifact, ProcessMode, SendMode, ServerEvent, ServerState, Signals,
    WorkflowJob, job_feed_event, signal_event, template, template_event,
};
use crate::{
    data::{HostRole, Uri},
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
};
use tokio::sync::mpsc;

struct JobDescriptor<'a> {
    id: u64,
    source: &'a str,
}
pub async fn run_job(
    state: Arc<ServerState>,
    signals: Signals,
    job_id: u64,
    request_user: String,
    tx: mpsc::Sender<ServerEvent>,
    job: WorkflowJob,
) {
    let cleanup = WorkflowCleanup::new(job.clone());
    let source = job.source().to_string();
    let validation = validate_workflow_request(&state, &signals, &job).await;
    if let Err(error) = validation {
        state.record_failure().await;
        send_event(
            &tx,
            template_event(template::JobFailed {
                job_id,
                error: &error.to_string(),
                source: &source,
            }),
        )
        .await;
        send_terminal_signal(&tx, &state).await;
        drop(cleanup);
        return;
    }

    let identifiers = merged_identifiers(
        job.identifiers.clone(),
        signals.metadata.clone(),
        request_user,
        &job.artifact,
    );

    let result = match &job.artifact {
        CollectedArtifact::LocalArchive { path, .. } => {
            execute_local_archive_job(
                state.clone(),
                &signals,
                job_id,
                path.clone(),
                &source,
                identifiers,
                &tx,
            )
            .await
        }
        CollectedArtifact::ServiceLink { uri, .. } => {
            execute_service_link_job(
                state.clone(),
                &signals,
                job_id,
                uri.clone(),
                &source,
                identifiers,
                &tx,
            )
            .await
        }
        CollectedArtifact::RemoteCollection {
            host,
            diagnostic_type,
            ..
        } => {
            execute_remote_collection_job(
                state.clone(),
                &signals,
                job_id,
                host.clone(),
                diagnostic_type.clone(),
                identifiers,
                &tx,
            )
            .await
        }
    };

    if let Err(error) = result {
        state.record_failure().await;
        send_event(
            &tx,
            template_event(template::JobFailed {
                job_id,
                error: &error.to_string(),
                source: &source,
            }),
        )
        .await;
    }

    send_terminal_signal(&tx, &state).await;
    drop(cleanup);
}

async fn execute_local_archive_job(
    state: Arc<ServerState>,
    signals: &Signals,
    job_id: u64,
    path: PathBuf,
    source: &str,
    identifiers: Identifiers,
    tx: &mpsc::Sender<ServerEvent>,
) -> Result<()> {
    match signals.workflow.process.mode {
        ProcessMode::Process => {
            let receiver = Arc::new(Receiver::try_from(Uri::File(path))?);
            let exporter = Arc::new(select_processed_exporter(state.clone(), signals).await?);
            let process_selection = explicit_process_selection(signals)?;
            run_processor_job(
                state,
                tx,
                receiver,
                exporter,
                identifiers,
                process_selection,
                JobDescriptor { id: job_id, source },
            )
            .await
        }
        ProcessMode::Forward => run_forward_job(state, tx, signals, job_id, source, &path).await,
    }
}

async fn execute_service_link_job(
    state: Arc<ServerState>,
    signals: &Signals,
    job_id: u64,
    uri: Uri,
    source: &str,
    identifiers: Identifiers,
    tx: &mpsc::Sender<ServerEvent>,
) -> Result<()> {
    if signals.workflow.collect.save {
        state.record_job_started().await;
        send_event(
            tx,
            job_feed_event(template::JobCollectionProcessing { job_id, source }),
        )
        .await;
        send_event(tx, signal_event(r#"{"loading":false,"processing":true}"#)).await;

        let collected =
            collect_service_link_archive(job_id, uri, source, signals, identifiers).await?;
        if let CollectedArtifact::LocalArchive { path, .. } = collected.artifact {
            state.record_success(0, 0).await;
            let archive_path = path.display().to_string();
            send_event(
                tx,
                template_event(template::JobCollectionCompleted {
                    job_id,
                    source,
                    archive_path: &archive_path,
                }),
            )
            .await;
            let handoff_job_id = new_job_id();
            return execute_local_archive_job(
                state,
                signals,
                handoff_job_id,
                path,
                source,
                collected.identifiers,
                tx,
            )
            .await;
        }

        return Err(eyre!(
            "Service link collection did not produce a local archive"
        ));
    }

    match signals.workflow.process.mode {
        ProcessMode::Process => {
            let receiver = Arc::new(Receiver::try_from(uri)?);
            let exporter = Arc::new(select_processed_exporter(state.clone(), signals).await?);
            let process_selection = explicit_process_selection(signals)?;
            run_processor_job(
                state,
                tx,
                receiver,
                exporter,
                identifiers,
                process_selection,
                JobDescriptor { id: job_id, source },
            )
            .await
        }
        ProcessMode::Forward => {
            let path = download_service_link_to_temp(&uri, job_id, source).await?;
            let _cleanup = LocalPathCleanup::new(path.clone());
            run_forward_job(state, tx, signals, job_id, source, &path).await
        }
    }
}

async fn execute_remote_collection_job(
    state: Arc<ServerState>,
    signals: &Signals,
    job_id: u64,
    host: crate::data::KnownHost,
    diagnostic_type: String,
    identifiers: Identifiers,
    tx: &mpsc::Sender<ServerEvent>,
) -> Result<()> {
    let source = host.get_url().to_string();
    if signals.workflow.process.mode == ProcessMode::Process && !signals.workflow.collect.save {
        let receiver = Arc::new(Receiver::try_from(host)?);
        let exporter = Arc::new(select_processed_exporter(state.clone(), signals).await?);
        let process_selection = explicit_process_selection(signals)?;
        return run_processor_job(
            state,
            tx,
            receiver,
            exporter,
            identifiers,
            process_selection,
            JobDescriptor {
                id: job_id,
                source: &source,
            },
        )
        .await;
    }

    if signals.workflow.collect.save {
        state.record_job_started().await;
        send_event(
            tx,
            job_feed_event(template::JobCollectionProcessing {
                job_id,
                source: &source,
            }),
        )
        .await;
        send_event(tx, signal_event(r#"{"loading":false,"processing":true}"#)).await;
    }

    let collected =
        collect_remote_archive(job_id, host, &diagnostic_type, signals, identifiers).await?;
    let _cleanup = match &collected.artifact {
        CollectedArtifact::LocalArchive {
            cleanup_path: Some(path),
            ..
        } => Some(LocalPathCleanup::new(path.clone())),
        _ => None,
    };

    if let CollectedArtifact::LocalArchive { path, .. } = collected.artifact {
        if signals.workflow.collect.save {
            state.record_success(0, 0).await;
            let archive_path = path.display().to_string();
            send_event(
                tx,
                template_event(template::JobCollectionCompleted {
                    job_id,
                    source: &source,
                    archive_path: &archive_path,
                }),
            )
            .await;
            let handoff_job_id = new_job_id();
            execute_local_archive_job(
                state,
                signals,
                handoff_job_id,
                path,
                &source,
                collected.identifiers,
                tx,
            )
            .await
        } else {
            execute_local_archive_job(
                state,
                signals,
                job_id,
                path,
                &source,
                collected.identifiers,
                tx,
            )
            .await
        }
    } else {
        Err(eyre!("Remote collection did not produce a local archive"))
    }
}

async fn run_processor_job(
    state: Arc<ServerState>,
    tx: &mpsc::Sender<ServerEvent>,
    receiver: Arc<Receiver>,
    exporter: Arc<Exporter>,
    identifiers: Identifiers,
    process_selection: Option<ProcessSelection>,
    job: JobDescriptor<'_>,
) -> Result<()> {
    let processor =
        Processor::try_new_with_selection(receiver, exporter, identifiers, process_selection)
            .await?;
    let processor = processor
        .start()
        .await
        .map_err(|failed| eyre!(failed.state.error))?;
    state.record_job_started().await;

    send_event(
        tx,
        job_feed_event(template::JobProcessing {
            job_id: job.id,
            source: job.source,
        }),
    )
    .await;
    send_event(tx, signal_event(r#"{"loading":false,"processing":true}"#)).await;

    match processor.process().await {
        Ok(completed) => {
            let report = &completed.state.report;
            state
                .record_success(report.diagnostic.docs.total, report.diagnostic.docs.errors)
                .await;
            send_event(
                tx,
                template_event(template::JobCompleted {
                    job_id: job.id,
                    diagnostic_id: &report.diagnostic.metadata.id,
                    docs_created: &report.diagnostic.docs.created,
                    duration: &format!(
                        "{:.3}",
                        report.diagnostic.processing_duration as f64 / 1000.0
                    ),
                    source: job.source,
                    kibana_link: report
                        .diagnostic
                        .kibana_link
                        .as_ref()
                        .unwrap_or(&"#".to_string()),
                    product: &report.diagnostic.product.to_string(),
                }),
            )
            .await;
            Ok(())
        }
        Err(failed) => Err(eyre!(failed.state.error)),
    }
}

fn explicit_process_selection(signals: &Signals) -> Result<Option<ProcessSelection>> {
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
    signals: &Signals,
    job_id: u64,
    source: &str,
    path: &Path,
) -> Result<()> {
    if signals.workflow.send.mode == SendMode::Local {
        send_event(
            tx,
            job_feed_event(template::JobForwardProcessing { job_id, source }),
        )
        .await;
        state.record_job_started().await;
        send_event(tx, signal_event(r#"{"loading":false,"processing":true}"#)).await;

        let destination = if !signals.workflow.collect.save_dir.trim().is_empty() {
            signals.workflow.collect.save_dir.trim().to_string()
        } else {
            path.display().to_string()
        };
        state.record_success(0, 0).await;
        send_event(
            tx,
            template_event(template::JobForwardCompleted {
                job_id,
                source,
                destination: &destination,
            }),
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

    send_event(
        tx,
        job_feed_event(template::JobForwardProcessing { job_id, source }),
    )
    .await;
    state.record_job_started().await;
    send_event(tx, signal_event(r#"{"loading":false,"processing":true}"#)).await;

    let response = uploader::upload_file(path, target, uploader::DEFAULT_UPLOAD_API_URL).await?;
    state.record_success(0, 0).await;
    let destination = format!("https://upload.elastic.co/g/{}", response.slug);
    send_event(
        tx,
        template_event(template::JobForwardCompleted {
            job_id,
            source,
            destination: &destination,
        }),
    )
    .await;
    Ok(())
}

async fn select_processed_exporter(state: Arc<ServerState>, signals: &Signals) -> Result<Exporter> {
    match signals.workflow.send.mode {
        SendMode::Remote => {
            let configured = state.exporter.read().await.clone();
            let configured_display = configured.to_string();
            let target = signals.workflow.send.remote_target.trim();
            if target.is_empty() || target == configured_display {
                Ok(configured)
            } else {
                Exporter::try_from(Uri::try_from(target.to_string())?)
            }
        }
        SendMode::Local => {
            let target = signals.workflow.send.local_target.trim();
            if target == "directory" {
                if !state.runtime_mode_policy.allows_local_artifacts() {
                    return Err(eyre!("Service mode does not allow local directory output"));
                }
                let directory = signals.workflow.send.local_directory.trim();
                if directory.is_empty() {
                    return Err(eyre!("Local directory output requires a directory path"));
                }
                Exporter::try_from(Uri::try_from(directory.to_string())?)
            } else if target.is_empty() {
                Err(eyre!(
                    "Local send requires a localhost host or local directory"
                ))
            } else {
                let uri = Uri::try_from(target.to_string())?;
                validate_local_send_uri(&uri)?;
                Exporter::try_from(uri)
            }
        }
    }
}

async fn validate_workflow_request(
    state: &ServerState,
    signals: &Signals,
    job: &WorkflowJob,
) -> Result<()> {
    if signals.workflow.collect.save && !state.runtime_mode_policy.allows_local_artifacts() {
        return Err(eyre!(
            "Service mode does not allow local bundle save artifacts"
        ));
    }

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
                job.artifact,
                CollectedArtifact::RemoteCollection { .. } | CollectedArtifact::ServiceLink { .. }
            ) && !signals.workflow.collect.save
            {
                return Err(eyre!(
                    "Forward + Local requires Save Archive in Collect so the bundle has a local destination"
                ));
            }

            if matches!(
                job.artifact,
                CollectedArtifact::LocalArchive {
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
            && !state.runtime_mode_policy.allows_local_artifacts()
        {
            return Err(eyre!("Service mode does not allow local directory output"));
        }
    }

    if matches!(
        job.artifact,
        CollectedArtifact::RemoteCollection { .. } | CollectedArtifact::ServiceLink { .. }
    ) && signals.workflow.collect.save
        && signals.workflow.collect.save_dir.trim().is_empty()
    {
        return Err(eyre!("Save Bundle requires a target directory"));
    }

    Ok(())
}

fn validate_local_send_uri(uri: &Uri) -> Result<()> {
    match uri {
        Uri::KnownHost(host) => {
            if !host.has_role(HostRole::Send) {
                return Err(eyre!(
                    "Local known-host send targets must have the `send` role"
                ));
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

async fn collect_remote_archive(
    job_id: u64,
    host: crate::data::KnownHost,
    diagnostic_type: &str,
    signals: &Signals,
    identifiers: Identifiers,
) -> Result<WorkflowJob> {
    let (output_dir, cleanup_path) = if signals.workflow.collect.save {
        if signals.workflow.collect.save_dir.trim().is_empty() {
            return Err(eyre!("Save Bundle requires a save directory"));
        }
        (
            PathBuf::from(signals.workflow.collect.save_dir.trim()),
            None,
        )
    } else {
        let temp_dir = std::env::temp_dir().join(format!("esdiag-workflow-{job_id}"));
        std::fs::create_dir_all(&temp_dir)?;
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
        artifact: CollectedArtifact::LocalArchive {
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
    signals: &Signals,
    identifiers: Identifiers,
) -> Result<WorkflowJob> {
    let filename = local_archive_filename(source, job_id)?;
    let (path, cleanup_path) = if signals.workflow.collect.save {
        if signals.workflow.collect.save_dir.trim().is_empty() {
            return Err(eyre!("Save Bundle requires a save directory"));
        }
        let directory = PathBuf::from(signals.workflow.collect.save_dir.trim());
        std::fs::create_dir_all(&directory)?;
        (directory.join(&filename), None)
    } else {
        let path = std::env::temp_dir().join(format!("esdiag-service-link-{job_id}-{filename}"));
        (path.clone(), Some(path))
    };

    download_service_link_to_path(&uri, &path).await?;

    Ok(WorkflowJob {
        identifiers: identifiers.with_filename(Some(filename.clone())),
        artifact: CollectedArtifact::LocalArchive {
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
        return Err(eyre!(
            "Expected an authenticated Elastic Upload Service URL"
        ));
    };

    let mut download_url = url.clone();
    let token = download_url
        .password()
        .ok_or_else(|| eyre!("Elastic Upload Service token is missing"))?
        .to_string();
    download_url.set_username("").ok();
    download_url.set_password(None).ok();

    let client = reqwest::Client::new();
    let response = client
        .get(download_url)
        .header("Authorization", token)
        .send()
        .await?;
    let bytes = response.bytes().await?;
    if bytes.is_empty() {
        return Err(eyre!("Downloaded empty file, check upload link expiration"));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
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

fn merged_identifiers(
    mut base: Identifiers,
    overrides: Identifiers,
    request_user: String,
    artifact: &CollectedArtifact,
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
    base.filename = overrides.filename.or_else(|| match artifact {
        CollectedArtifact::LocalArchive { filename, .. } => Some(filename.clone()),
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
            r#"{{"loading":false,"processing":false,"stats":{}}}"#,
            state.get_stats().await
        )),
    )
    .await;
}

struct WorkflowCleanup(Option<WorkflowJob>);

impl WorkflowCleanup {
    fn new(job: WorkflowJob) -> Self {
        Self(Some(job))
    }
}

impl Drop for WorkflowCleanup {
    fn drop(&mut self) {
        if let Some(job) = self.0.take() {
            job.cleanup();
        }
    }
}

struct LocalPathCleanup(PathBuf);

impl LocalPathCleanup {
    fn new(path: PathBuf) -> Self {
        Self(path)
    }
}

impl Drop for LocalPathCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_local_send_uri, validate_workflow_request};
    use crate::{
        data::{HostRole, KnownHostBuilder, Uri},
        exporter::Exporter,
        server::{
            CollectedArtifact, ProcessMode, RuntimeMode, RuntimeModePolicy, SendMode, ServerEvent,
            ServerState, Signals, Stats, WorkflowJob,
        },
    };
    use std::{collections::HashMap, sync::Arc};
    use tokio::sync::{RwLock, broadcast, watch};
    use url::Url;

    fn test_state(mode: RuntimeMode) -> ServerState {
        let (stats_updates_tx, stats_updates_rx) = watch::channel(0u64);
        ServerState {
            exporter: Arc::new(RwLock::new(Exporter::default())),
            kibana_url: Arc::new(RwLock::new(String::new())),
            signals: Arc::new(RwLock::new(Signals::default())),
            workflow_jobs: Arc::new(RwLock::new(HashMap::new())),
            runtime_mode: mode,
            runtime_mode_policy: RuntimeModePolicy::new(mode),
            keystore_state: Arc::new(RwLock::new(HashMap::new())),
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
    async fn service_mode_rejects_bundle_save() {
        let state = test_state(RuntimeMode::Service);
        let mut signals = Signals::default();
        signals.workflow.collect.save = true;
        signals.workflow.collect.save_dir = "/tmp".to_string();

        let job = WorkflowJob {
            identifiers: Default::default(),
            artifact: CollectedArtifact::LocalArchive {
                source: "upload.zip".to_string(),
                filename: "upload.zip".to_string(),
                path: "/tmp/upload.zip".into(),
                cleanup_path: None,
            },
        };

        assert!(
            validate_workflow_request(&state, &signals, &job)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn service_link_save_requires_directory() {
        let state = test_state(RuntimeMode::User);
        let mut signals = Signals::default();
        signals.workflow.collect.save = true;

        let job = WorkflowJob {
            identifiers: Default::default(),
            artifact: CollectedArtifact::ServiceLink {
                source: "downloaded.zip".to_string(),
                uri: Uri::ServiceLink(
                    Url::parse("https://token:secret@example.com/archive.zip").unwrap(),
                ),
            },
        };

        assert!(
            validate_workflow_request(&state, &signals, &job)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn forward_local_temp_upload_requires_save_server_side() {
        let state = test_state(RuntimeMode::User);
        let mut signals = Signals::default();
        signals.workflow.process.mode = ProcessMode::Forward;
        signals.workflow.send.mode = SendMode::Local;

        let job = WorkflowJob {
            identifiers: Default::default(),
            artifact: CollectedArtifact::LocalArchive {
                source: "upload.zip".to_string(),
                filename: "upload.zip".to_string(),
                path: "/tmp/upload.zip".into(),
                cleanup_path: Some("/tmp/upload.zip".into()),
            },
        };

        assert!(
            validate_workflow_request(&state, &signals, &job)
                .await
                .is_err()
        );
    }
}
