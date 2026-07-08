// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! The `Job` stage: the universal model of one diagnostic execution
//! (ADR-0002/0003/0004) and the one executor that drives it, plus the
//! saved-job command handlers shared by the CLI (`esdiag job run`) and the
//! web server.

/// The one executor: derives staged vs streaming and drives the stages.
pub mod executor;
/// The phase-structured `Job` model with validated construction.
pub mod model;

use crate::data::{HostRole, Job as SavedJob, KnownHost, load_saved_jobs, save_saved_jobs};
use crate::job::model::Input;
use eyre::{Result, eyre};
use std::io::IsTerminal;

pub fn handle_job_list() -> Result<()> {
    let jobs = load_saved_jobs()?;
    if jobs.is_empty() {
        return Ok(());
    }

    let hosts = KnownHost::parse_hosts_yml()?;

    #[allow(clippy::literal_string_with_formatting_args)]
    let header = format!(
        "{:<24} {:<24} {:<16} {}",
        "Name", "Collection target", "Processing", "Send target"
    );
    println!("{header}");
    let separator: String = "-".repeat(80);
    println!("{separator}");
    let use_color = std::io::stdout().is_terminal();

    for (name, job) in &jobs {
        let collect_target = job.collect_host();
        let stale = !collect_target.is_empty() && !hosts.contains_key(collect_target);
        let collect_display = if stale && use_color {
            format!("\x1b[31m{collect_target}\x1b[0m")
        } else {
            collect_target.to_string()
        };

        let processing = job.processing_label();
        let send_target = job.send_target_label();

        println!(
            "{:<24} {:<24} {:<16} {}",
            name, collect_display, processing, send_target
        );
    }

    Ok(())
}

pub async fn handle_job_run(name: &str) -> Result<()> {
    let jobs = load_saved_jobs()?;
    let job = jobs.get(name).ok_or_else(|| eyre!("Saved job '{}' not found", name))?;

    if let Input::Collect { host, .. } = job.input() {
        let host_key = host.trim();
        if host_key.is_empty() {
            return Err(eyre!("Saved job '{}' has no collection host configured", name));
        }

        let hosts = KnownHost::parse_hosts_yml()?;
        let Some(known_host) = hosts.get(host_key) else {
            return Err(eyre!(
                "Host '{}' referenced by job '{}' not found in hosts.yml",
                host_key,
                name
            ));
        };
        if !known_host.has_role(HostRole::Collect) {
            return Err(eyre!(
                "Host role validation failed for job '{}': host '{}' is missing the collect role",
                name,
                host_key
            ));
        }
    }

    tracing::info!("Running saved job '{name}'");
    run_job(job.clone()).await?;
    tracing::info!("Saved job '{name}' completed successfully");
    Ok(())
}

pub fn handle_job_delete(name: &str) -> Result<()> {
    let mut jobs = load_saved_jobs()?;
    if jobs.shift_remove(name).is_none() {
        return Err(eyre!("Saved job '{}' not found", name));
    }
    save_saved_jobs(&jobs)?;
    tracing::info!("Deleted saved job '{name}'");
    Ok(())
}

pub fn save_job(name: &str, job: SavedJob) -> Result<()> {
    validate_saved_job_name(name)?;
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
pub async fn run_job(job: SavedJob) -> Result<()> {
    match job.input() {
        Input::Collect { host, .. } => tracing::info!("Running saved collect job against {host}"),
        Input::Load { uri } => tracing::info!("Running saved load job from {uri}"),
    }

    let outcome = executor::execute(job).await?;
    if let (Some(bundle_path), true) = (&outcome.bundle_path, outcome.bundle_retained) {
        tracing::info!("Retained collected bundle: {}", bundle_path.display());
    }
    Ok(())
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
