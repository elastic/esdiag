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

use super::model::{ExecutionMode, ExportTarget, Input, Job, Process, SendTarget};
use crate::{
    data::{Uri, collect_product},
    exporter::Exporter,
    processor::{Collector, Identifiers, Processor, api::ProcessSelection},
    receiver::Receiver,
    uploader,
};
use eyre::{Result, eyre};
use std::{path::PathBuf, sync::Arc};

/// What one job execution produced.
#[derive(Debug, Default)]
pub struct JobOutcome {
    /// The materialised bundle, when the job saved or loaded one.
    pub bundle_path: Option<PathBuf>,
    /// Whether that bundle outlives the job (a retained `save`, or a `Load`
    /// input) as opposed to a temporary staging bundle.
    pub bundle_retained: bool,
    /// The upload slug returned by the Elastic Uploader for a `Send` stage.
    pub upload_slug: Option<String>,
    /// Whether a `Process` stage ran to completion.
    pub processed: bool,
}

/// Execute one job: resolve the Phase-1 input, honor the derived mode
/// (staged vs streaming), and run the selected stages. Phase 3 is and/or —
/// `Export` (inside `Process`) and `Send` may both run in one job.
pub async fn execute(job: Job) -> Result<JobOutcome> {
    let mut outcome = JobOutcome::default();

    match job.input() {
        Input::Collect {
            host,
            diagnostic_type,
            include,
            exclude,
        } => {
            match job.execution_mode() {
                ExecutionMode::Staged => {
                    // `Save` is the serialization barrier: collection
                    // completes and the bundle materialises before any
                    // processing reads it.
                    let save = job
                        .save()
                        .ok_or_else(|| eyre!("staged Collect job without save (unreachable by construction)"))?;
                    let (output_dir, cleanup) = match &save.dir {
                        Some(dir) => (dir.clone(), None),
                        None => temp_bundle_dir()?,
                    };
                    outcome.bundle_retained = save.is_retained();

                    let receiver = Receiver::try_from((**host).clone())?;
                    let product = collect_product(host.app())?;
                    let collect_exporter = Exporter::for_collect_archive(output_dir)?;
                    let collector = Collector::try_new(
                        receiver,
                        collect_exporter,
                        product,
                        diagnostic_type.clone(),
                        include.clone(),
                        exclude.clone(),
                        job.identifiers.clone(),
                    )
                    .await?;
                    let result = collector.collect().await?;
                    let bundle_path = PathBuf::from(&result.path);
                    tracing::info!("Collected bundle: {}", bundle_path.display());

                    if let Some(process) = job.process() {
                        run_process(
                            Receiver::try_from(Uri::File(bundle_path.clone()))?,
                            process,
                            job.identifiers.clone(),
                        )
                        .await?;
                        outcome.processed = true;
                    }
                    if let Some(send) = job.send() {
                        outcome.upload_slug = Some(run_send(&bundle_path, send).await?);
                    }

                    outcome.bundle_path = Some(bundle_path);
                    drop(cleanup);
                }
                ExecutionMode::Streaming => {
                    // No `Save`: receive, transform, and export overlap. The
                    // processor consumes the live receiver directly.
                    let process = job
                        .process()
                        .ok_or_else(|| eyre!("streaming job without process (unreachable by construction)"))?;
                    let _ = collect_product(host.app())?;
                    run_process(Receiver::try_from((**host).clone())?, process, job.identifiers.clone()).await?;
                    outcome.processed = true;
                }
            }
        }
        Input::Load { uri } => {
            if let Some(process) = job.process() {
                run_process(Receiver::try_from(uri.clone())?, process, job.identifiers.clone()).await?;
                outcome.processed = true;
            }
            if let Some(send) = job.send() {
                let bundle_path = match uri {
                    Uri::File(path) => path.clone(),
                    other => {
                        return Err(eyre!(
                            "`send` requires a bundle archive file; input '{other}' is not sendable"
                        ));
                    }
                };
                outcome.upload_slug = Some(run_send(&bundle_path, send).await?);
                outcome.bundle_path = Some(bundle_path);
                outcome.bundle_retained = true;
            }
        }
    }

    Ok(outcome)
}

/// Run the `Process` stage (with its `Export` sink) over the given input
/// receiver — a materialised bundle in staged mode, a live receiver in
/// streaming mode.
async fn run_process(receiver: Receiver, process: &Process, identifiers: Identifiers) -> Result<()> {
    let exporter = export_target_exporter(&process.export)?;
    let selection: Option<ProcessSelection> = process.selection.clone();
    let processor =
        Processor::try_new_with_selection(Arc::new(receiver), Arc::new(exporter), identifiers, selection).await?;
    let processing = processor.start().await.map_err(|failed| eyre!("{}", failed))?;
    match processing.process().await {
        Ok(completed) => {
            tracing::info!("Processing complete in {:.3}s", completed.state.runtime as f64 / 1000.0);
            Ok(())
        }
        Err(failed) => Err(eyre!("{}", failed)),
    }
}

/// Run the `Send` stage: transmit an existing bundle to the Elastic Uploader.
async fn run_send(bundle_path: &std::path::Path, send: &SendTarget) -> Result<String> {
    let response = uploader::upload_file(bundle_path, &send.upload_id, uploader::DEFAULT_UPLOAD_API_URL).await?;
    tracing::info!("Forwarded bundle to https://upload.elastic.co/g/{}", response.slug);
    Ok(response.slug)
}

/// Resolve the `Export` sink for processed documents.
fn export_target_exporter(target: &ExportTarget) -> Result<Exporter> {
    match target {
        ExportTarget::KnownHost { name } => Exporter::try_from(Uri::try_from(name.clone())?),
        ExportTarget::File { path } => Exporter::try_from(Uri::try_from(path.display().to_string())?),
        ExportTarget::Directory { dir } => Exporter::try_from(Uri::try_from(dir.display().to_string())?),
        ExportTarget::Stdout => Exporter::try_from(Uri::Stream),
    }
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
    use crate::processor::Identifiers;
    use std::fs::File;
    use zip::ZipArchive;

    fn fixture_archive(name: &str) -> PathBuf {
        PathBuf::from(format!("{}/tests/archives/{name}", env!("CARGO_MANIFEST_DIR")))
    }

    fn extract_fixture(name: &str, destination: &std::path::Path) {
        std::fs::create_dir_all(destination).expect("create dir");
        let file = File::open(fixture_archive(name)).expect("open fixture");
        let mut archive = ZipArchive::new(file).expect("read fixture");
        archive.extract(destination).expect("extract fixture");
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
                    dir: output.path().to_path_buf(),
                },
            }),
            None,
        )
        .expect("valid load+process job");

        let outcome = execute(job).await.expect("job executes");
        assert!(outcome.processed);
        assert!(outcome.upload_slug.is_none());
        // Processing produced exported document streams
        let produced = std::fs::read_dir(output.path()).expect("read output dir").count();
        assert!(produced > 0, "expected exported document files");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn load_send_job_requires_an_archive_file() {
        let bundle = tempfile::tempdir().expect("bundle dir");

        let job = Job::try_new(
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
        .expect("valid load+send job shape");

        let err = execute(job).await.expect_err("directory input is not sendable");
        assert!(err.to_string().contains("not sendable"));
    }
}
