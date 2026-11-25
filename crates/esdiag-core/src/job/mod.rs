// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! The `Job` stage: the universal model of one diagnostic execution
//! (ADR-0002/0003/0004) and the one executor that drives it, plus the
//! saved-job command handlers shared by the CLI (`esdiag job run`) and the
//! web server.

/// Execution-only resources, identity, retention policy, and observers.
pub mod context;
/// The one executor: derives staged vs streaming and drives the stages.
pub mod executor;
/// The phase-structured `Job` model with validated construction.
pub mod model;
/// Structured per-stage results and typed lifecycle events.
pub mod outcome;

pub use executor::{FailedStage, JobExecutionFailure, JobOutcome};

use crate::data::{Job as SavedJob, load_saved_jobs, save_saved_jobs};
use crate::job::model::Input;
use eyre::{Result, eyre};

/// A saved-job lookup failed before execution began.
///
/// The CLI may expose the name safely as structured `resource` context while
/// keeping other error details out of its public output contract.
#[derive(Debug)]
pub struct SavedJobNotFound {
    pub name: String,
}

impl std::fmt::Display for SavedJobNotFound {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Saved job '{}' not found", self.name)
    }
}

impl std::error::Error for SavedJobNotFound {}

pub fn load_saved_job(name: &str) -> Result<SavedJob> {
    load_saved_jobs()?
        .shift_remove(name)
        .ok_or_else(|| eyre::Report::new(SavedJobNotFound { name: name.to_string() }))
}

/// Runs a job loaded by [`load_saved_job`], so checks made on that value hold
/// for the run even if `jobs.yml` changes meanwhile.
pub async fn run_saved_job(name: &str, job: SavedJob) -> Result<JobOutcome> {
    if let Input::Collect { host, .. } = job.input() {
        let host_key = host.trim();
        if host_key.is_empty() {
            return Err(eyre!("Saved job '{}' has no collection host configured", name));
        }
    }

    tracing::info!("Running saved job '{name}'");
    let outcome = run_job(job).await?;
    tracing::info!("Saved job '{name}' completed successfully");
    Ok(outcome)
}

pub fn handle_job_delete(name: &str) -> Result<()> {
    let mut jobs = load_saved_jobs()?;
    if jobs.shift_remove(name).is_none() {
        return Err(SavedJobNotFound { name: name.to_string() }.into());
    }
    save_saved_jobs(&jobs)?;
    tracing::info!("Deleted saved job '{name}'");
    Ok(())
}

pub fn save_job(name: &str, job: SavedJob) -> Result<()> {
    validate_saved_job_name(name)?;
    job.validate_for_persistence()?;
    let mut jobs = load_saved_jobs()?;
    jobs.insert(name.trim().to_string(), job);
    save_saved_jobs(&jobs)?;
    tracing::info!("Saved job '{name}'");
    Ok(())
}

pub fn validate_saved_job_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(eyre!("Job name cannot be empty"));
    }
    if trimmed
        .chars()
        .any(|ch| ch.is_control() || matches!(ch, '/' | '\\' | '?' | '#' | '%'))
    {
        return Err(eyre!("Job name contains unsupported path characters"));
    }
    Ok(())
}

/// Run a saved phase-structured job definition with the one executor.
pub async fn run_job(job: SavedJob) -> Result<JobOutcome> {
    match job.input() {
        Input::Collect { host, .. } => tracing::info!("Running saved collect job against {host}"),
        Input::Load { uri } => tracing::info!("Running saved load job from {uri}"),
        Input::CollectBinding { binding, .. } | Input::LoadBinding { binding } => {
            return Err(eyre!(
                "Saved job contains runtime input binding '{}'; runtime bindings cannot be persisted",
                binding.as_str()
            ));
        }
    }

    let outcome = executor::execute(job).await?;
    if let (Some(bundle_path), true) = (&outcome.bundle_path, outcome.bundle_retained) {
        tracing::info!("Retained collected bundle: {}", bundle_path.display());
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::validate_saved_job_name;
    use crate::data::Job as SavedJob;
    use crate::job::model::{ExecutionMode, ExportTarget, Input, Process, SaveTarget};
    use std::path::PathBuf;

    fn saved_collect_job() -> SavedJob {
        SavedJob::try_new(
            Default::default(),
            Input::Collect {
                host: "prod".to_string(),
                diagnostic_type: "standard".to_string(),
                include: None,
                exclude: None,
            },
            Some(SaveTarget::retained(PathBuf::from("/tmp/esdiag-saved-jobs"))),
            None,
            None,
        )
        .expect("valid saved job")
    }

    #[test]
    fn job_builder_creates_collect_job() {
        let job = saved_collect_job();

        assert_eq!(job.send_target_label(), "dir:/tmp/esdiag-saved-jobs");
    }

    #[test]
    fn process_job_without_save_is_streaming() {
        let job = SavedJob::try_new(
            Default::default(),
            Input::Collect {
                host: "prod".to_string(),
                diagnostic_type: "standard".to_string(),
                include: None,
                exclude: None,
            },
            None,
            Some(Process {
                selection: None,
                export: ExportTarget::Stdout,
            }),
            None,
        )
        .expect("streaming process job");

        assert!(matches!(job.input(), Input::Collect { .. }));
        assert!(job.save().is_none());
        assert!(job.process().is_some());
        assert_eq!(job.execution_mode(), ExecutionMode::Streaming);
    }

    #[test]
    fn validate_saved_job_name_rejects_empty_names() {
        assert_eq!(
            validate_saved_job_name("   ")
                .expect_err("empty names should be rejected")
                .to_string(),
            "Job name cannot be empty"
        );
    }

    #[test]
    fn validate_saved_job_name_rejects_path_unsafe_characters() {
        assert_eq!(
            validate_saved_job_name("bad/job")
                .expect_err("path characters should be rejected")
                .to_string(),
            "Job name contains unsupported path characters"
        );
    }
}
