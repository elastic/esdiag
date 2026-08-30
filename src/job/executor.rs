// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! The one job executor (ADR-0002/0004): derives the execution mode from the
//! job's stage selection and drives the stages. Both the CLI and the web
//! build a [`Job`](super::model::Job) and hand it here.
//!
//! Staging note: the executor currently composes the existing collection and
//! processing machinery (`Collector`, `Processor`) behind the unified model,
//! per the design's landing strategy — the model and executor land first,
//! the legacy types retire once every surface drives this path.

use super::model::{ExecutionMode, Input, Job, Process};
use super::{
    context::{ExecutionContext, ExecutionIdentity, RetentionPolicy},
    outcome::{ChildExecutionOutcome, ExecutionEvent, ExecutionOutcome, Lifecycle, SendResult, Stage, StageStatus},
};
use crate::{
    data::{Application, Platform, Uri},
    exporter::BundleExporter,
    processor::{
        CollectOptions, DiagnosticOutcome, Identifiers,
        api::{ApiResolver, ProcessSelection},
        collect_bundle, process_documents, skip_kind_for, skipped_application,
    },
    receiver::Receiver,
};
use eyre::{Result, eyre};
use futures::{StreamExt, stream::FuturesUnordered};
use std::{path::PathBuf, sync::Arc};

/// What one job execution produced.
#[derive(Default)]
pub struct JobOutcome {
    /// A retained local bundle archive path available after execution.
    ///
    /// Temporary staging bundles are omitted because their directory is removed
    /// before the outcome reaches the caller.
    pub bundle_path: Option<PathBuf>,
    /// Whether `bundle_path` points at a retained archive-file bundle as
    /// opposed to a temporary staging bundle.
    pub bundle_retained: bool,
    /// Whether the retained bundle was produced by this job's collection stage.
    pub bundle_created: bool,
    /// The destination slug returned by the Elastic Upload Service for a `Send` stage.
    pub send_slug: Option<String>,
    /// Whether a `Process` stage ran to completion.
    pub processed: bool,
    /// The complete execution outcome used to report saved-job results.
    pub execution: Option<ExecutionOutcome>,
}

/// The stage that failed after a job may already have produced durable output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailedStage {
    Collect,
    Process,
    Send,
}

/// Preserves the safe, completed portion of a failed saved-job execution.
pub struct JobExecutionFailure {
    pub stage: FailedStage,
    pub outcome: JobOutcome,
    message: String,
    source: eyre::Report,
}

impl std::fmt::Debug for JobExecutionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JobExecutionFailure")
            .field("stage", &self.stage)
            .field("message", &self.message)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for JobExecutionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for JobExecutionFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

fn job_outcome(outcome: ExecutionOutcome) -> JobOutcome {
    JobOutcome {
        bundle_path: outcome.retained_bundle.clone(),
        bundle_retained: outcome.retained_bundle.is_some(),
        bundle_created: outcome.collection.is_some(),
        send_slug: outcome.send.as_ref().map(|send| send.slug.clone()),
        processed: outcome.report.is_some(),
        execution: Some(outcome),
    }
}

fn failed_stage(outcome: &ExecutionOutcome) -> FailedStage {
    outcome
        .stages
        .iter()
        .find(|stage| stage.status.is_unsuccessful())
        .map(|stage| match stage.stage {
            Stage::Send => FailedStage::Send,
            Stage::Process | Stage::Export => FailedStage::Process,
            Stage::Collect | Stage::Load | Stage::Save => FailedStage::Collect,
        })
        .unwrap_or(FailedStage::Process)
}

fn failed(stage: FailedStage, outcome: JobOutcome, error: eyre::Report) -> eyre::Report {
    eyre::Report::new(JobExecutionFailure {
        stage,
        outcome,
        message: error.to_string(),
        source: error,
    })
}

/// Execute one job: resolve the Phase-1 input, honor the derived mode
/// (staged vs streaming), and run the selected stages. Phase 3 is and/or —
/// `Export` (inside `Process`) and `Send` may both run in one job.
pub async fn execute(job: Job) -> Result<JobOutcome> {
    let outcome = execute_with_context(job, ExecutionContext::default()).await;
    if !outcome.succeeded() {
        let failures = outcome
            .stages
            .iter()
            .filter_map(|stage| match &stage.status {
                StageStatus::Failed(error) => Some(format!("{:?}: {error}", stage.stage)),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("; ");
        let stage = failed_stage(&outcome);
        return Err(failed(
            stage,
            job_outcome(outcome),
            eyre!("Job execution failed: {failures}"),
        ));
    }
    Ok(job_outcome(outcome))
}

pub async fn execute_with_context(job: Job, context: ExecutionContext) -> ExecutionOutcome {
    let mut outcome = ExecutionOutcome::new(context.identity.clone());
    let input_stage = if job.input().is_collect() {
        Stage::Collect
    } else {
        Stage::Load
    };
    context.observe(ExecutionEvent::new(
        context.identity.clone(),
        input_stage,
        Lifecycle::Queued,
    ));
    context.observe(ExecutionEvent::new(
        context.identity.clone(),
        input_stage,
        Lifecycle::Started,
    ));

    let materialize_remote = !job.input().is_collect()
        && (job.process().is_some()
            || job.send().is_some()
            || context.retention == RetentionPolicy::RetainLoadedBundle);
    let require_local_bundle = !job.input().is_collect() && job.send().is_some();
    let mut resolved = match context
        .inputs
        .resolve(job.input(), materialize_remote, require_local_bundle)
        .await
    {
        Ok(resolved) => resolved,
        Err(error) => {
            record_stage(
                &context,
                &mut outcome,
                input_stage,
                StageStatus::Failed(error.to_string()),
            );
            record_blocked_outputs(&context, &job, &mut outcome, "Phase-1 input failed");
            return outcome;
        }
    };
    let derived_selection = if job.input().is_collect() {
        job.process()
            .map(|process| {
                collect_process_selection(
                    resolved.application.as_ref().unwrap_or(&Application::Agent),
                    collect_diagnostic_type(job.input()),
                    collect_include(job.input()),
                    collect_exclude(job.input()),
                    process,
                )
            })
            .transpose()
            .map(|selection| selection.flatten())
            .map_err(|error| error.to_string())
    } else {
        Ok(None)
    };

    if job.execution_mode() == ExecutionMode::Streaming {
        record_stage(&context, &mut outcome, Stage::Collect, StageStatus::Succeeded);
        let Some(process) = job.process() else {
            record_blocked_outputs(&context, &job, &mut outcome, "Streaming execution requires Process");
            return outcome;
        };
        let derived_selection = match derived_selection {
            Ok(selection) => selection,
            Err(error) => {
                record_stage(&context, &mut outcome, Stage::Process, StageStatus::Failed(error));
                record_stage(
                    &context,
                    &mut outcome,
                    Stage::Export,
                    StageStatus::Blocked("Process selection is invalid".to_string()),
                );
                return outcome;
            }
        };
        run_process_stage(
            resolved.receiver.clone(),
            process,
            job.identifiers.clone(),
            derived_selection,
            &context,
            &mut outcome,
        )
        .await;
        return outcome;
    }

    let mut staging_cleanup = None;
    if job.input().is_collect() {
        let Some(save) = job.save() else {
            record_stage(
                &context,
                &mut outcome,
                Stage::Collect,
                StageStatus::Failed("Staged Collect requires Save".to_string()),
            );
            record_blocked_outputs(&context, &job, &mut outcome, "Collect did not produce a bundle");
            return outcome;
        };
        let (output_dir, cleanup) = match &save.dir {
            Some(dir) => (dir.clone(), None),
            None => match temp_bundle_dir() {
                Ok(temp) => temp,
                Err(error) => {
                    record_stage(
                        &context,
                        &mut outcome,
                        Stage::Save,
                        StageStatus::Failed(error.to_string()),
                    );
                    record_blocked_outputs(&context, &job, &mut outcome, "Save setup failed");
                    return outcome;
                }
            },
        };
        staging_cleanup = cleanup;
        context.observe(ExecutionEvent::new(
            context.identity.clone(),
            Stage::Save,
            Lifecycle::Queued,
        ));
        context.observe(ExecutionEvent::new(
            context.identity.clone(),
            Stage::Save,
            Lifecycle::Started,
        ));
        let Some(application) = resolved.application else {
            record_stage(
                &context,
                &mut outcome,
                Stage::Collect,
                StageStatus::Failed("Collect input did not resolve an application".to_string()),
            );
            record_blocked_outputs(&context, &job, &mut outcome, "Collect application resolution failed");
            return outcome;
        };
        let exporter = match BundleExporter::archive(output_dir) {
            Ok(exporter) => exporter,
            Err(error) => {
                record_stage(
                    &context,
                    &mut outcome,
                    Stage::Save,
                    StageStatus::Failed(error.to_string()),
                );
                record_blocked_outputs(&context, &job, &mut outcome, "Save setup failed");
                return outcome;
            }
        };
        let collection = match collect_bundle(
            resolved.receiver,
            exporter,
            CollectOptions {
                application,
                r#type: collect_diagnostic_type(job.input()).to_string(),
                include: collect_include(job.input()).cloned(),
                exclude: collect_exclude(job.input()).cloned(),
                identifiers: job.identifiers.clone(),
            },
        )
        .await
        {
            Ok(collection) => collection,
            Err(error) => {
                record_stage(
                    &context,
                    &mut outcome,
                    Stage::Collect,
                    StageStatus::Failed(error.to_string()),
                );
                record_stage(
                    &context,
                    &mut outcome,
                    Stage::Save,
                    StageStatus::Failed(error.to_string()),
                );
                record_blocked_outputs(&context, &job, &mut outcome, "Collect failed");
                return outcome;
            }
        };
        record_stage(&context, &mut outcome, Stage::Collect, StageStatus::Succeeded);
        record_stage(&context, &mut outcome, Stage::Save, StageStatus::Succeeded);
        let bundle_path = PathBuf::from(&collection.path);
        outcome.collection = Some(collection);
        if save.is_retained() {
            outcome.retained_bundle = Some(bundle_path.clone());
        }
        resolved = match Receiver::try_from(Uri::File(bundle_path.clone())) {
            Ok(receiver) => crate::receiver::ResolvedInput::from_bundle(receiver, bundle_path),
            Err(error) => {
                record_blocked_outputs(
                    &context,
                    &job,
                    &mut outcome,
                    &format!("Saved bundle could not be loaded: {error}"),
                );
                return outcome;
            }
        };
    } else {
        record_stage(&context, &mut outcome, Stage::Load, StageStatus::Succeeded);
        if context.retention == RetentionPolicy::RetainLoadedBundle {
            outcome.retained_bundle = resolved.retain_bundle();
        } else if matches!(job.input(), Input::Load { uri: Uri::File(_) }) {
            outcome.retained_bundle = resolved.bundle_path.clone();
        }
    }

    run_independent_outputs(&job, &context, &mut outcome, &resolved, derived_selection).await;
    drop(staging_cleanup);
    outcome
}

async fn run_independent_outputs(
    job: &Job,
    context: &ExecutionContext,
    outcome: &mut ExecutionOutcome,
    resolved: &crate::receiver::ResolvedInput,
    derived_selection: std::result::Result<Option<ProcessSelection>, String>,
) {
    let process = job.process().cloned();
    let process_receiver = resolved.receiver.clone();
    let process_identifiers = job.identifiers.clone();
    let process_context = context.clone();
    let send = job.send().cloned();
    let send_path = resolved.bundle_path.clone();
    let send_context = context.clone();

    let process_future = async move {
        match process {
            Some(process) => Some(match derived_selection {
                Ok(derived_selection) => {
                    run_process_result(
                        process_receiver,
                        &process,
                        process_identifiers,
                        derived_selection,
                        &process_context,
                    )
                    .await
                }
                Err(error) => Err(eyre!(error)),
            }),
            None => None,
        }
    };
    let send_future = async move {
        match (send, send_path) {
            (Some(send), Some(path)) => {
                send_context.observe(ExecutionEvent::new(
                    send_context.identity.clone(),
                    Stage::Send,
                    Lifecycle::Queued,
                ));
                send_context.observe(ExecutionEvent::new(
                    send_context.identity.clone(),
                    Stage::Send,
                    Lifecycle::Started,
                ));
                send_context.observe(
                    ExecutionEvent::new(send_context.identity.clone(), Stage::Send, Lifecycle::Progress)
                        .with_message("Sending resolved bundle"),
                );
                Some(
                    send_context
                        .sender
                        .send(&path, &send.upload_id)
                        .await
                        .map(|response| SendResult { slug: response.slug }),
                )
            }
            (Some(_), None) => Some(Err(eyre!("Send requires a resolved local bundle"))),
            (None, _) => None,
        }
    };

    let (process_result, send_result) = tokio::join!(process_future, send_future);
    apply_process_result(context, outcome, process_result);
    if let Some(send_result) = send_result {
        match send_result {
            Ok(upload) => {
                outcome.send = Some(upload);
                record_stage(context, outcome, Stage::Send, StageStatus::Succeeded);
            }
            Err(error) => record_stage(context, outcome, Stage::Send, StageStatus::Failed(error.to_string())),
        }
    }
}

async fn run_process_stage(
    receiver: Receiver,
    process: &Process,
    identifiers: Identifiers,
    derived_selection: Option<ProcessSelection>,
    context: &ExecutionContext,
    outcome: &mut ExecutionOutcome,
) {
    let result = run_process_result(receiver, process, identifiers, derived_selection, context).await;
    apply_process_result(context, outcome, Some(result));
}

struct ProcessExecution {
    report: crate::processor::DiagnosticReport,
    children: Vec<ChildExecutionOutcome>,
    export_error: Option<String>,
}

async fn run_process_result(
    receiver: Receiver,
    process: &Process,
    identifiers: Identifiers,
    derived_selection: Option<ProcessSelection>,
    context: &ExecutionContext,
) -> Result<ProcessExecution> {
    context.observe(ExecutionEvent::new(
        context.identity.clone(),
        Stage::Process,
        Lifecycle::Queued,
    ));
    context.observe(ExecutionEvent::new(
        context.identity.clone(),
        Stage::Process,
        Lifecycle::Started,
    ));
    context.observe(
        ExecutionEvent::new(context.identity.clone(), Stage::Process, Lifecycle::Progress)
            .with_message("Transforming diagnostic"),
    );
    context.observe(ExecutionEvent::new(
        context.identity.clone(),
        Stage::Export,
        Lifecycle::Queued,
    ));
    context.observe(ExecutionEvent::new(
        context.identity.clone(),
        Stage::Export,
        Lifecycle::Started,
    ));
    let exporter = context.resolve_document_exporter(&process.export)?;
    let selection = process
        .selection
        .clone()
        .or(derived_selection)
        .map(canonicalize_process_selection)
        .transpose()?;
    let parent_receiver = receiver.clone();
    let processed = process_documents(
        Arc::new(receiver),
        exporter,
        identifiers.clone(),
        selection,
        context.child_depth == 0,
    )
    .await?;
    let report = processed.report;
    let children = run_child_jobs(
        processed.included_diagnostics,
        parent_receiver,
        process,
        identifiers
            .with_parent_id(report.diagnostic.metadata.id.clone())
            .with_platform(report.diagnostic.platform()),
        report.diagnostic.platform(),
        context,
    )
    .await;
    Ok(ProcessExecution {
        report,
        children,
        export_error: processed.export_error,
    })
}

async fn run_child_jobs(
    declarations: Vec<crate::processor::diagnostic::DiagPath>,
    parent_receiver: Receiver,
    parent_process: &Process,
    identifiers: Identifiers,
    inherited_platform: Platform,
    context: &ExecutionContext,
) -> Vec<ChildExecutionOutcome> {
    let children = FuturesUnordered::new();
    for declaration in declarations {
        let path = declaration.diag_path;
        let process = Process {
            selection: None,
            export: parent_process.export.clone(),
        };
        let identifiers = identifiers.clone();
        let parent_receiver = parent_receiver.clone();
        let child_context = context.child(inherited_platform);
        children.push(async move {
            let started = std::time::Instant::now();
            let mut child_context = match child_context {
                Ok(context) => context,
                Err(error) => {
                    let identity = ExecutionIdentity {
                        job_id: crate::processor::new_job_id(),
                        owner: context.identity.owner.clone(),
                        parent_job_id: Some(context.identity.job_id),
                    };
                    return failed_child_execution(identity, path, error.to_string(), inherited_platform);
                }
            };
            let child_identity = child_context.identity.clone();
            let child_job_id = child_identity.job_id;
            let binding = match super::model::BindingKey::try_new(format!("included-{child_job_id}")) {
                Ok(binding) => binding,
                Err(error) => {
                    return failed_child_execution(child_identity, path, error.to_string(), inherited_platform);
                }
            };
            child_context
                .inputs
                .bind_nested(binding.clone(), parent_receiver, path.clone(), None);
            let job = match Job::try_new(identifiers, Input::LoadBinding { binding }, None, Some(process), None) {
                Ok(job) => job,
                Err(error) => {
                    return failed_child_execution(child_identity, path, error.to_string(), inherited_platform);
                }
            };
            let outcome = Box::pin(execute_with_context(job, child_context)).await;
            let error = outcome.stages.iter().find_map(|stage| match &stage.status {
                StageStatus::Failed(error) | StageStatus::Blocked(error) => Some(error.as_str()),
                StageStatus::Succeeded | StageStatus::Skipped(_) => None,
            });
            let diagnostic_outcome = outcome
                .diagnostic_outcome()
                .or_else(|| error.and_then(skip_kind_for).map(DiagnosticOutcome::Skipped))
                .unwrap_or(DiagnosticOutcome::Failed);
            let application = outcome
                .report
                .as_ref()
                .and_then(|report| report.diagnostic.application)
                .or_else(|| error.and_then(skipped_application));
            let platform = outcome
                .report
                .as_ref()
                .map(|report| report.diagnostic.platform())
                .unwrap_or(inherited_platform);
            let runtime = outcome.report.as_ref().map(|_| started.elapsed().as_millis());
            ChildExecutionOutcome {
                path,
                execution: Box::new(outcome),
                diagnostic_outcome,
                application,
                platform,
                runtime,
            }
        });
    }
    children.collect().await
}

fn failed_child_execution(
    identity: ExecutionIdentity,
    path: String,
    error: String,
    platform: Platform,
) -> ChildExecutionOutcome {
    let mut execution = ExecutionOutcome::new(identity);
    execution.record(Stage::Load, StageStatus::Failed(error));
    ChildExecutionOutcome {
        path,
        execution: Box::new(execution),
        diagnostic_outcome: DiagnosticOutcome::Failed,
        application: None,
        platform,
        runtime: None,
    }
}

fn apply_process_result(
    context: &ExecutionContext,
    outcome: &mut ExecutionOutcome,
    result: Option<Result<ProcessExecution>>,
) {
    let Some(result) = result else {
        return;
    };
    match result {
        Ok(processed) => {
            outcome.report = Some(processed.report);
            outcome.children = processed.children;
            record_stage(context, outcome, Stage::Process, StageStatus::Succeeded);
            match processed.export_error {
                Some(error) => record_stage(context, outcome, Stage::Export, StageStatus::Failed(error)),
                None => record_stage(context, outcome, Stage::Export, StageStatus::Succeeded),
            }
        }
        Err(error) => {
            record_stage(context, outcome, Stage::Process, StageStatus::Failed(error.to_string()));
            record_stage(
                context,
                outcome,
                Stage::Export,
                StageStatus::Blocked("Process did not complete".to_string()),
            );
        }
    }
}

fn record_stage(context: &ExecutionContext, outcome: &mut ExecutionOutcome, stage: Stage, status: StageStatus) {
    if outcome.stage(stage).is_some() {
        return;
    }
    let message = match &status {
        StageStatus::Failed(message) | StageStatus::Blocked(message) | StageStatus::Skipped(message) => {
            Some(message.clone())
        }
        StageStatus::Succeeded => None,
    };
    let mut event = ExecutionEvent::new(context.identity.clone(), stage, Lifecycle::Completed);
    if let Some(message) = message {
        event = event.with_message(message);
    }
    context.observe(event);
    outcome.record(stage, status);
}

fn record_blocked_outputs(context: &ExecutionContext, job: &Job, outcome: &mut ExecutionOutcome, reason: &str) {
    if job.process().is_some() {
        if outcome.stage(Stage::Process).is_none() {
            record_stage(
                context,
                outcome,
                Stage::Process,
                StageStatus::Blocked(reason.to_string()),
            );
        }
        if outcome.stage(Stage::Export).is_none() {
            record_stage(
                context,
                outcome,
                Stage::Export,
                StageStatus::Blocked(reason.to_string()),
            );
        }
    }
    if job.send().is_some() && outcome.stage(Stage::Send).is_none() {
        record_stage(context, outcome, Stage::Send, StageStatus::Blocked(reason.to_string()));
    }
}

fn collect_diagnostic_type(input: &Input) -> &str {
    match input {
        Input::Collect { diagnostic_type, .. }
        | Input::CollectContext { diagnostic_type, .. }
        | Input::CollectBinding { diagnostic_type, .. } => diagnostic_type,
        Input::Load { .. } | Input::LoadBinding { .. } => "standard",
    }
}

fn collect_include(input: &Input) -> Option<&Vec<String>> {
    match input {
        Input::Collect { include, .. }
        | Input::CollectContext { include, .. }
        | Input::CollectBinding { include, .. } => include.as_ref(),
        Input::Load { .. } | Input::LoadBinding { .. } => None,
    }
}

fn collect_exclude(input: &Input) -> Option<&Vec<String>> {
    match input {
        Input::Collect { exclude, .. }
        | Input::CollectContext { exclude, .. }
        | Input::CollectBinding { exclude, .. } => exclude.as_ref(),
        Input::Load { .. } | Input::LoadBinding { .. } => None,
    }
}

fn canonicalize_process_selection(selection: ProcessSelection) -> Result<ProcessSelection> {
    let selected =
        ApiResolver::resolve_processing_selection(&selection.product, &selection.diagnostic_type, &selection.selected)?;
    Ok(ProcessSelection { selected, ..selection })
}

fn collect_process_selection(
    application: &Application,
    diagnostic_type: &str,
    include: Option<&Vec<String>>,
    exclude: Option<&Vec<String>>,
    process: &Process,
) -> Result<Option<ProcessSelection>> {
    let product = match application {
        Application::Elasticsearch => "elasticsearch",
        Application::Logstash => "logstash",
        _ => return Ok(None),
    };
    if let Some(selection) = &process.selection {
        ApiResolver::validate_processing_selection_with_collect_filters(
            product,
            diagnostic_type,
            include,
            exclude,
            &selection.selected,
        )?;
        return Ok(None);
    }

    let selected =
        ApiResolver::resolve_processing_selection_with_collect_filters(product, diagnostic_type, include, exclude)?;
    Ok(Some(ProcessSelection {
        product: product.to_string(),
        diagnostic_type: diagnostic_type.to_string(),
        selected,
    }))
}

fn temp_bundle_dir() -> Result<(PathBuf, Option<TempDirCleanup>)> {
    let temp_dir = std::env::temp_dir().join(format!("esdiag-job-{}", uuid::Uuid::new_v4().as_u64_pair().0));
    std::fs::create_dir_all(&temp_dir)?;
    Ok((temp_dir.clone(), Some(TempDirCleanup(temp_dir))))
}

struct TempDirCleanup(PathBuf);

impl Drop for TempDirCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elastic_upload_service::{BundleSending, FinalizeResponse};
    use crate::job::{
        context::{ExecutionIdentity, ExecutionObserver},
        model::{BindingKey, ExportTarget, SendTarget},
    };
    use crate::processor::Identifiers;
    use futures::future::BoxFuture;
    use std::{
        fs::File,
        sync::{Arc, Mutex},
    };
    use zip::ZipArchive;

    #[derive(Debug)]
    struct SourceError;

    impl std::fmt::Display for SourceError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("source error")
        }
    }

    impl std::error::Error for SourceError {}

    fn fixture_archive(name: &str) -> PathBuf {
        PathBuf::from(format!("{}/tests/archives/{name}", env!("CARGO_MANIFEST_DIR")))
    }

    fn extract_fixture(name: &str, destination: &std::path::Path) {
        std::fs::create_dir_all(destination).expect("create dir");
        let file = File::open(fixture_archive(name)).expect("open fixture");
        let mut archive = ZipArchive::new(file).expect("read fixture");
        archive.extract(destination).expect("extract fixture");
    }

    fn parent_with_children(children: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("parent dir");
        let included: Vec<crate::processor::diagnostic::DiagPath> = children
            .iter()
            .map(|(path, archive)| {
                extract_fixture(archive, &root.path().join(path));
                crate::processor::diagnostic::DiagPath {
                    diag_type: "diagnostic".to_string(),
                    diag_path: (*path).to_string(),
                }
            })
            .collect();
        let manifest = serde_json::json!({
            "mode": "support",
            "product": "eck",
            "flags": null,
            "diagnostic": "esdiag-test",
            "type": "eck-diagnostics",
            "runner": "esdiag",
            "version": "3.0.0",
            "timestamp": "2026-01-01T00:00:00Z",
            "collection_date_millis": 1767225600000u64,
            "included_diagnostics": included,
            "identifiers": null,
            "requested_apis": null,
            "collected_apis": null
        });
        std::fs::write(
            root.path().join(crate::processor::DiagnosticManifest::FILENAME),
            serde_json::to_vec_pretty(&manifest).expect("serialize parent manifest"),
        )
        .expect("write parent manifest");
        root
    }

    #[derive(Default)]
    struct RecordingObserver(Mutex<Vec<ExecutionEvent>>);

    impl ExecutionObserver for RecordingObserver {
        fn observe(&self, event: &ExecutionEvent) {
            self.0.lock().expect("events lock").push(event.clone());
        }
    }

    #[derive(Clone, Copy)]
    struct TestSender {
        fail: bool,
    }

    impl BundleSending for TestSender {
        fn send<'a>(
            &'a self,
            _bundle_path: &'a std::path::Path,
            _upload_id: &'a str,
        ) -> BoxFuture<'a, Result<FinalizeResponse>> {
            Box::pin(async move {
                if self.fail {
                    Err(eyre!("send failed"))
                } else {
                    Ok(FinalizeResponse {
                        slug: "raw-bundle".to_string(),
                        token: "redacted-test-token".to_string(),
                    })
                }
            })
        }
    }

    fn unavailable_document_exporter() -> crate::exporter::DocumentExporter {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve port");
        let address = listener.local_addr().expect("listener address");
        drop(listener);
        let host = crate::data::KnownHost::new_no_auth(
            Application::Elasticsearch,
            url::Url::parse(&format!("http://{address}")).expect("output URL"),
            vec![crate::data::HostRole::Send],
            None,
            false,
        );
        crate::exporter::DocumentExporter::try_from(host).expect("document exporter")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn load_process_job_processes_an_existing_bundle() {
        let bundle = tempfile::tempdir().expect("bundle dir");
        extract_fixture("elasticsearch-api-diagnostics-9.3.3.zip", bundle.path());
        let output = tempfile::tempdir().expect("output dir");

        let job = Job::try_new(
            Identifiers::default(),
            Input::Load {
                uri: Uri::Directory(bundle.path().to_path_buf()),
            },
            None,
            Some(Process {
                selection: None,
                export: ExportTarget::Directory {
                    output_dir: output.path().to_path_buf(),
                },
            }),
            None,
        )
        .expect("valid load+process job");

        let outcome = execute(job).await.expect("job executes");
        assert!(outcome.processed);
        assert!(outcome.send_slug.is_none());
        // Processing produced exported document streams
        let produced = std::fs::read_dir(output.path()).expect("read output dir").count();
        assert!(produced > 0, "expected exported document files");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn structured_outcome_preserves_report_and_observer_identity() {
        let bundle = tempfile::tempdir().expect("bundle dir");
        extract_fixture("elasticsearch-api-diagnostics-9.3.3.zip", bundle.path());
        let output = tempfile::tempdir().expect("output dir");
        let job = Job::try_new(
            Identifiers::default(),
            Input::Load {
                uri: Uri::Directory(bundle.path().to_path_buf()),
            },
            None,
            Some(Process {
                selection: None,
                export: ExportTarget::Directory {
                    output_dir: output.path().to_path_buf(),
                },
            }),
            None,
        )
        .expect("load process job");
        let observer = Arc::new(RecordingObserver::default());
        let identity = ExecutionIdentity::new(42, "alice@example.com");
        let context = ExecutionContext::default()
            .with_identity(identity.clone())
            .with_observer(observer.clone());

        let outcome = execute_with_context(job, context).await;

        assert!(outcome.succeeded(), "stage outcomes: {:?}", outcome.stages);
        assert_eq!(outcome.identity, identity);
        assert!(outcome.report.is_some());
        assert_eq!(outcome.stage(Stage::Load), Some(&StageStatus::Succeeded));
        assert_eq!(outcome.stage(Stage::Process), Some(&StageStatus::Succeeded));
        assert_eq!(outcome.stage(Stage::Export), Some(&StageStatus::Succeeded));
        let events = observer.0.lock().expect("events lock");
        assert!(!events.is_empty());
        assert!(events.iter().all(|event| event.identity == identity));
        assert!(events.iter().any(|event| event.lifecycle == Lifecycle::Progress));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn staged_collect_serializes_before_raw_send_starts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock Elasticsearch listener");
        let address = listener.local_addr().expect("listener address");
        let app = axum::Router::new().fallback(axum::routing::get(|| async {
            axum::Json(serde_json::json!({
                "name": "test-node",
                "cluster_name": "test-cluster",
                "cluster_uuid": "test-cluster-id",
                "version": {"number": "9.3.3"},
                "tagline": "You Know, for Search"
            }))
        }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("mock server");
        });
        let retained = tempfile::tempdir().expect("retained bundle directory");
        let binding = BindingKey::try_new("staged-collect-source").expect("binding");
        let job = Job::try_new(
            Identifiers::default(),
            Input::CollectBinding {
                binding: binding.clone(),
                diagnostic_type: "standard".to_string(),
                include: Some(vec!["cluster_settings".to_string()]),
                exclude: None,
            },
            Some(crate::job::model::SaveTarget::retained(retained.path().to_path_buf())),
            None,
            Some(SendTarget {
                upload_id: "upload-123".to_string(),
            }),
        )
        .expect("staged collect job");
        let observer = Arc::new(RecordingObserver::default());
        let mut context = ExecutionContext::default()
            .with_observer(observer.clone())
            .with_sender(TestSender { fail: false });
        let host = crate::data::KnownHost::new_no_auth(
            Application::Elasticsearch,
            url::Url::parse(&format!("http://{address}")).expect("mock URL"),
            vec![crate::data::HostRole::Collect],
            None,
            false,
        );
        context.inputs.bind_receiver(
            binding,
            Receiver::try_from(host).expect("source receiver"),
            Some(Application::Elasticsearch),
        );

        let outcome = execute_with_context(job, context).await;

        assert!(outcome.succeeded(), "stage outcomes: {:?}", outcome.stages);
        assert_eq!(outcome.stage(Stage::Save), Some(&StageStatus::Succeeded));
        assert_eq!(outcome.stage(Stage::Send), Some(&StageStatus::Succeeded));
        let events = observer.0.lock().expect("events lock");
        let save_completed = events
            .iter()
            .position(|event| event.stage == Stage::Save && event.lifecycle == Lifecycle::Completed)
            .expect("Save completed event");
        let send_started = events
            .iter()
            .position(|event| event.stage == Stage::Send && event.lifecycle == Lifecycle::Started)
            .expect("Send started event");
        assert!(
            save_completed < send_started,
            "raw Send must not start before staged collection is serialized"
        );
        server.abort();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn default_context_executes_saved_job_with_stable_input_and_save_target() {
        let _environment = crate::TestEnv::new();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock Elasticsearch listener");
        let address = listener.local_addr().expect("listener address");
        let app = axum::Router::new().fallback(axum::routing::get(|| async {
            axum::Json(serde_json::json!({
                "name": "test-node",
                "cluster_name": "test-cluster",
                "cluster_uuid": "test-cluster-id",
                "version": {"number": "9.3.3"},
                "tagline": "You Know, for Search"
            }))
        }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("mock server");
        });
        crate::data::KnownHost::new_no_auth(
            Application::Elasticsearch,
            url::Url::parse(&format!("http://{address}")).expect("mock URL"),
            vec![crate::data::HostRole::Collect],
            None,
            false,
        )
        .save("stable-collect")
        .expect("save known host");
        let output = tempfile::tempdir().expect("stable save directory");
        let job = Job::try_new(
            Identifiers::default(),
            Input::Collect {
                host: "stable-collect".to_string(),
                diagnostic_type: "standard".to_string(),
                include: Some(vec!["cluster_settings".to_string()]),
                exclude: None,
            },
            Some(crate::job::model::SaveTarget::retained(output.path().to_path_buf())),
            None,
            None,
        )
        .expect("saved compatible job");

        let outcome = execute(job).await.expect("default context executes saved job");

        assert!(outcome.bundle_retained);
        assert!(
            outcome
                .bundle_path
                .as_ref()
                .is_some_and(|path| path.starts_with(output.path()))
        );
        server.abort();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn included_diagnostics_execute_as_owner_scoped_child_jobs() {
        let bundle = parent_with_children(&[
            ("child-a", "elasticsearch-api-diagnostics-9.3.3.zip"),
            ("child-b", "elasticsearch-api-diagnostics-8.19.3.zip"),
        ]);
        let output = tempfile::tempdir().expect("output");
        let job = Job::try_new(
            Identifiers::default(),
            Input::Load {
                uri: Uri::Directory(bundle.path().to_path_buf()),
            },
            None,
            Some(Process {
                selection: None,
                export: ExportTarget::Directory {
                    output_dir: output.path().to_path_buf(),
                },
            }),
            None,
        )
        .expect("parent process job");
        let observer = Arc::new(RecordingObserver::default());
        let context = ExecutionContext::default()
            .with_identity(ExecutionIdentity::new(42, "alice@example.com"))
            .with_observer(observer.clone());

        let outcome = execute_with_context(job, context).await;

        assert!(outcome.succeeded());
        assert_eq!(outcome.children.len(), 2);
        let parent_id = outcome
            .report
            .as_ref()
            .expect("parent report")
            .diagnostic
            .metadata
            .id
            .clone();
        let child_ids: std::collections::HashSet<_> =
            outcome.children.iter().map(ChildExecutionOutcome::job_id).collect();
        assert_eq!(child_ids.len(), 2);
        assert!(!child_ids.contains(&42));
        for child in &outcome.children {
            let report = child.report().expect("child report");
            assert_eq!(child.platform(), Platform::ECK);
            assert_eq!(child.execution.identity.parent_job_id, Some(42));
            assert_eq!(child.execution.stage(Stage::Load), Some(&StageStatus::Succeeded));
            assert_eq!(child.execution.stage(Stage::Process), Some(&StageStatus::Succeeded));
            assert_eq!(child.execution.stage(Stage::Export), Some(&StageStatus::Succeeded));
            assert_eq!(
                report.diagnostic.identifiers.parent_id.as_deref(),
                Some(parent_id.as_str())
            );
        }
        let events = observer.0.lock().expect("events lock");
        for child_id in child_ids {
            assert!(events.iter().any(|event| {
                event.identity.job_id == child_id
                    && event.identity.owner == "alice@example.com"
                    && event.identity.parent_job_id == Some(42)
            }));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn child_processing_gap_remains_a_typed_skip() {
        let bundle = parent_with_children(&[("kibana-child", "kibana-api-diagnostics-9.3.3.zip")]);
        let output = tempfile::tempdir().expect("output");
        let job = Job::try_new(
            Identifiers::default(),
            Input::Load {
                uri: Uri::Directory(bundle.path().to_path_buf()),
            },
            None,
            Some(Process {
                selection: None,
                export: ExportTarget::Directory {
                    output_dir: output.path().to_path_buf(),
                },
            }),
            None,
        )
        .expect("parent process job");

        let outcome = execute_with_context(job, ExecutionContext::default()).await;

        assert!(outcome.succeeded());
        let child = outcome.children.first().expect("child outcome");
        assert_eq!(
            child.diagnostic_outcome,
            DiagnosticOutcome::Skipped(crate::processor::SkipKind::NotImplemented)
        );
        assert_eq!(child.application(), Some(Application::Kibana));
        assert_eq!(
            child.execution_error(),
            Some("Kibana processing is not yet implemented")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn one_child_failure_does_not_erase_successful_sibling() {
        let bundle = parent_with_children(&[
            ("child-ok", "elasticsearch-api-diagnostics-9.3.3.zip"),
            ("child-missing", "elasticsearch-api-diagnostics-8.19.3.zip"),
        ]);
        std::fs::remove_dir_all(bundle.path().join("child-missing")).expect("remove child fixture");
        let output = tempfile::tempdir().expect("output");
        let job = Job::try_new(
            Identifiers::default(),
            Input::Load {
                uri: Uri::Directory(bundle.path().to_path_buf()),
            },
            None,
            Some(Process {
                selection: None,
                export: ExportTarget::Directory {
                    output_dir: output.path().to_path_buf(),
                },
            }),
            None,
        )
        .expect("parent process job");

        let outcome = execute_with_context(job, ExecutionContext::default()).await;

        assert!(outcome.succeeded(), "child failures do not fail the parent");
        assert_eq!(outcome.children.len(), 2);
        assert!(outcome.children.iter().any(|child| child.report().is_some()));
        assert!(
            outcome
                .children
                .iter()
                .any(|child| child.diagnostic_outcome == DiagnosticOutcome::Failed)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn included_diagnostic_fan_out_stays_one_level_deep() {
        let bundle = parent_with_children(&[("child-es", "elasticsearch-api-diagnostics-9.3.3.zip")]);
        let child_manifest_path = bundle
            .path()
            .join("child-es")
            .join(crate::processor::DiagnosticManifest::FILENAME);
        let mut child_manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&child_manifest_path).expect("read child manifest"))
                .expect("parse child manifest");
        child_manifest["included_diagnostics"] = serde_json::json!([
            {"diag_type": "diagnostic", "diag_path": "grandchild"}
        ]);
        std::fs::write(
            child_manifest_path,
            serde_json::to_vec_pretty(&child_manifest).expect("serialize child manifest"),
        )
        .expect("write child manifest");
        let output = tempfile::tempdir().expect("output");
        let job = Job::try_new(
            Identifiers::default(),
            Input::Load {
                uri: Uri::Directory(bundle.path().to_path_buf()),
            },
            None,
            Some(Process {
                selection: None,
                export: ExportTarget::Directory {
                    output_dir: output.path().to_path_buf(),
                },
            }),
            None,
        )
        .expect("parent process job");

        let outcome = execute_with_context(job, ExecutionContext::default()).await;

        assert!(outcome.succeeded());
        assert_eq!(outcome.children.len(), 1, "grandchild jobs must not be spawned");
        assert!(
            outcome.children[0].report().is_some() || outcome.children[0].execution_error().is_some(),
            "the direct child outcome must be retained even when it cannot report"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn child_export_failure_preserves_child_diagnostic_verdict() {
        let bundle = parent_with_children(&[("child-es", "elasticsearch-api-diagnostics-9.3.3.zip")]);
        let output_binding = BindingKey::try_new("child-failing-export").expect("binding");
        let job = Job::try_new(
            Identifiers::default(),
            Input::Load {
                uri: Uri::Directory(bundle.path().to_path_buf()),
            },
            None,
            Some(Process {
                selection: None,
                export: ExportTarget::Binding {
                    binding: output_binding.clone(),
                },
            }),
            None,
        )
        .expect("parent process job");
        let mut context = ExecutionContext::default();
        context.bind_document_exporter(output_binding, unavailable_document_exporter());

        let outcome = execute_with_context(job, context).await;

        assert!(outcome.succeeded(), "child failure must not fail the parent");
        let child = outcome.children.first().expect("child outcome");
        let report = child.report().expect("completed child report");
        assert_eq!(child.diagnostic_outcome, report.outcome());
        assert_eq!(child.diagnostic_outcome, DiagnosticOutcome::Complete);
        assert!(
            child.export_error().is_some(),
            "the child Export failure must remain separately available"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn export_failure_preserves_completed_diagnostic_report() {
        let bundle = tempfile::tempdir().expect("bundle dir");
        extract_fixture("elasticsearch-api-diagnostics-9.3.3.zip", bundle.path());
        let output_binding = BindingKey::try_new("failing-export").expect("binding");
        let job = Job::try_new(
            Identifiers::default(),
            Input::Load {
                uri: Uri::Directory(bundle.path().to_path_buf()),
            },
            None,
            Some(Process {
                selection: None,
                export: ExportTarget::Binding {
                    binding: output_binding.clone(),
                },
            }),
            None,
        )
        .expect("load process job");
        let mut context = ExecutionContext::default();
        context.bind_document_exporter(output_binding, unavailable_document_exporter());

        let outcome = execute_with_context(job, context).await;

        assert!(
            outcome.report.is_some(),
            "diagnostic report must survive export failure"
        );
        assert_eq!(outcome.stage(Stage::Process), Some(&StageStatus::Succeeded));
        assert!(matches!(outcome.stage(Stage::Export), Some(StageStatus::Failed(_))));
        assert!(!outcome.succeeded());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn export_and_send_outcomes_are_independent_for_every_result_pair() {
        for (export_fails, send_fails) in [(false, false), (false, true), (true, false), (true, true)] {
            let output = tempfile::tempdir().expect("output");
            let output_binding = BindingKey::try_new(format!("output-{export_fails}-{send_fails}")).expect("binding");
            let job = Job::try_new(
                Identifiers::default(),
                Input::Load {
                    uri: Uri::File(fixture_archive("elasticsearch-api-diagnostics-9.3.3.zip")),
                },
                None,
                Some(Process {
                    selection: None,
                    export: ExportTarget::Binding {
                        binding: output_binding.clone(),
                    },
                }),
                Some(SendTarget {
                    upload_id: "upload-123".to_string(),
                }),
            )
            .expect("load process send job");
            let exporter = if export_fails {
                unavailable_document_exporter()
            } else {
                crate::exporter::DocumentExporter::try_from(Uri::Directory(output.path().to_path_buf()))
                    .expect("directory exporter")
            };
            let mut context = ExecutionContext::default().with_sender(TestSender { fail: send_fails });
            context.bind_document_exporter(output_binding, exporter);

            let outcome = execute_with_context(job, context).await;

            assert!(outcome.report.is_some(), "report must survive output failures");
            assert_eq!(
                matches!(outcome.stage(Stage::Export), Some(StageStatus::Failed(_))),
                export_fails
            );
            assert_eq!(
                matches!(outcome.stage(Stage::Send), Some(StageStatus::Failed(_))),
                send_fails
            );
            assert_eq!(outcome.send.is_some(), !send_fails);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn raw_send_can_succeed_after_process_failure() {
        let bundle = tempfile::Builder::new()
            .suffix(".zip")
            .tempfile()
            .expect("empty archive");
        zip::ZipWriter::new(bundle.reopen().expect("archive handle"))
            .finish()
            .expect("finish archive");
        let output = tempfile::tempdir().expect("output");
        let job = Job::try_new(
            Identifiers::default(),
            Input::Load {
                uri: Uri::File(bundle.path().to_path_buf()),
            },
            None,
            Some(Process {
                selection: None,
                export: ExportTarget::Directory {
                    output_dir: output.path().to_path_buf(),
                },
            }),
            Some(SendTarget {
                upload_id: "upload-123".to_string(),
            }),
        )
        .expect("load process send job");

        let outcome =
            execute_with_context(job, ExecutionContext::default().with_sender(TestSender { fail: false })).await;

        assert!(matches!(outcome.stage(Stage::Process), Some(StageStatus::Failed(_))));
        assert!(matches!(outcome.stage(Stage::Export), Some(StageStatus::Blocked(_))));
        assert_eq!(outcome.stage(Stage::Send), Some(&StageStatus::Succeeded));
        assert_eq!(outcome.send.as_ref().map(|send| send.slug.as_str()), Some("raw-bundle"));
        assert!(!outcome.succeeded());
    }

    #[tokio::test]
    async fn input_failure_blocks_every_selected_output() {
        let binding = BindingKey::try_new("missing-upload").expect("binding key");
        let job = Job::try_new(
            Identifiers::default(),
            Input::LoadBinding { binding },
            None,
            Some(Process {
                selection: None,
                export: ExportTarget::Stdout,
            }),
            Some(SendTarget {
                upload_id: "abc123".to_string(),
            }),
        )
        .expect("runtime load can select process and send");

        let outcome = execute_with_context(job, ExecutionContext::default()).await;

        assert!(matches!(outcome.stage(Stage::Load), Some(StageStatus::Failed(_))));
        assert!(matches!(outcome.stage(Stage::Process), Some(StageStatus::Blocked(_))));
        assert!(matches!(outcome.stage(Stage::Export), Some(StageStatus::Blocked(_))));
        assert!(matches!(outcome.stage(Stage::Send), Some(StageStatus::Blocked(_))));
        assert!(!outcome.succeeded());
    }

    #[test]
    fn load_send_job_requires_an_archive_file() {
        let bundle = tempfile::tempdir().expect("bundle dir");

        let err = Job::try_new(
            Identifiers::default(),
            Input::Load {
                uri: Uri::Directory(bundle.path().to_path_buf()),
            },
            None,
            None,
            Some(SendTarget {
                upload_id: "abc123".to_string(),
            }),
        )
        .expect_err("directory input is not sendable");
        assert!(err.to_string().contains("requires a local bundle archive file"));
    }

    #[test]
    fn process_selection_is_canonicalized_before_execution() {
        let selection = canonicalize_process_selection(ProcessSelection {
            product: "logstash".to_string(),
            diagnostic_type: "standard".to_string(),
            selected: vec!["node_stats".to_string()],
        })
        .expect("canonical selection");

        assert!(selection.selected.contains(&"logstash_node_stats".to_string()));
        assert!(selection.selected.contains(&"logstash_node".to_string()));
        // `logstash_version` is a collect-only prerequisite with no processor.
        assert!(!selection.selected.contains(&"logstash_version".to_string()));
    }

    #[test]
    fn failed_job_preserves_source_error_for_downcasting() {
        let report = failed(
            FailedStage::Process,
            JobOutcome::default(),
            eyre::Report::new(SourceError),
        );

        assert!(
            report
                .chain()
                .any(|cause| cause.downcast_ref::<SourceError>().is_some())
        );
    }
}
