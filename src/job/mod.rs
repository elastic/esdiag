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

use crate::{
    data::{Job as SavedJob, JobAction, JobOutput, JobProcessSelection, KnownHost, load_saved_jobs, save_saved_jobs},
    processor::api::{ApiResolver, ProcessSelection},
};
use eyre::{Result, eyre};
use model::{ExportTarget, Input, Job, Process, SaveTarget, SendTarget};
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

    let host_name = job.collect_host();
    if host_name.is_empty() {
        return Err(eyre!("Saved job '{}' has no collection host configured", name));
    }

    let hosts = KnownHost::parse_hosts_yml()?;
    let host = hosts.get(host_name).cloned().ok_or_else(|| {
        eyre!(
            "Host '{}' referenced by job '{}' not found in hosts.yml",
            host_name,
            name
        )
    })?;

    tracing::info!("Running saved job '{name}'");
    run_job(job.clone(), host).await?;
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

/// Run a saved job definition: build the unified phase-structured [`Job`]
/// from the legacy `{collect, action}` shape and hand it to the one executor.
///
/// (The on-disk migration of `jobs.yml` to the phase shape is owned by
/// `saved-job-migration`; this converts at the execution boundary.)
pub async fn run_job(job: SavedJob, host: KnownHost) -> Result<()> {
    let host_url = host.get_url()?.to_string();
    tracing::info!("Running saved job against {host_url}");

    let unified = unify_saved_job(&job, host)?;
    let outcome = executor::execute(unified).await?;
    if let (Some(bundle_path), true) = (&outcome.bundle_path, outcome.bundle_retained) {
        tracing::info!("Retained collected bundle: {}", bundle_path.display());
    }
    Ok(())
}

/// Map the legacy `{collect, action}` saved-job shape onto the unified
/// phase-structured model. Every legacy action collects and materialises a
/// bundle first (always staged) — that behavior is preserved here; streaming
/// shapes become expressible for newly authored jobs.
fn unify_saved_job(job: &SavedJob, host: KnownHost) -> Result<Job> {
    let input = Input::Collect {
        host: Box::new(host),
        diagnostic_type: job.collect.diagnostic_type.clone(),
        include: None,
        exclude: None,
    };

    let retained_save = job.collect.save_dir.clone().map(SaveTarget::retained);

    let (save, process, send) = match &job.action {
        JobAction::Collect { output_dir } => (Some(SaveTarget::retained(output_dir.clone())), None, None),
        JobAction::Upload { upload_id } => (
            Some(retained_save.unwrap_or_else(SaveTarget::temporary)),
            None,
            Some(SendTarget {
                upload_id: upload_id.clone(),
            }),
        ),
        JobAction::Process { output, selection } => (
            Some(retained_save.unwrap_or_else(SaveTarget::temporary)),
            Some(Process {
                selection: explicit_process_selection(selection.as_ref())?,
                export: export_target(output),
            }),
            None,
        ),
    };

    Job::try_new(job.identifiers.clone(), input, save, process, send).map_err(Into::into)
}

fn export_target(output: &JobOutput) -> ExportTarget {
    match output {
        JobOutput::KnownHost { name } => ExportTarget::KnownHost { name: name.clone() },
        JobOutput::File { path } => ExportTarget::File { path: path.clone() },
        JobOutput::Directory { output_dir } => ExportTarget::Directory {
            dir: output_dir.clone(),
        },
        JobOutput::Stdout => ExportTarget::Stdout,
    }
}

fn explicit_process_selection(selection: Option<&JobProcessSelection>) -> Result<Option<ProcessSelection>> {
    let Some(selection) = selection else {
        return Ok(None);
    };
    let selected =
        ApiResolver::resolve_processing_selection(&selection.product, &selection.diagnostic_type, &selection.selected)?;

    Ok(Some(ProcessSelection {
        product: selection.product.clone(),
        diagnostic_type: selection.diagnostic_type.clone(),
        selected,
    }))
}

#[cfg(test)]
mod tests {
    use super::{explicit_process_selection, unify_saved_job, validate_saved_job_name};
    use crate::data::{Job as SavedJob, JobProcessSelection, KnownHostBuilder};
    use crate::job::model::{ExecutionMode, Input};
    use std::path::PathBuf;
    use url::Url;

    fn host() -> crate::data::KnownHost {
        KnownHostBuilder::new(Url::parse("http://localhost:9200").expect("url"))
            .build()
            .expect("host")
    }

    #[test]
    fn job_builder_creates_collect_job() {
        let job = SavedJob {
            identifiers: Default::default(),
            collect: crate::data::JobCollect {
                host: "prod".to_string(),
                diagnostic_type: "standard".to_string(),
                save_dir: None,
            },
            action: crate::data::JobAction::Collect {
                output_dir: PathBuf::from("/tmp/esdiag-saved-jobs"),
            },
        };

        assert_eq!(job.send_target_label(), "dir:/tmp/esdiag-saved-jobs");
    }

    #[test]
    fn explicit_process_selection_uses_job_values() {
        let selection = JobProcessSelection {
            product: "logstash".to_string(),
            diagnostic_type: "standard".to_string(),
            selected: vec!["node".to_string()],
        };

        let selection = explicit_process_selection(Some(&selection))
            .expect("selection")
            .expect("saved selection");

        assert_eq!(selection.product, "logstash");
        // Legacy short keys canonicalize to registry keys (ADR-0005)
        assert!(selection.selected.iter().any(|item| item == "logstash_node"));
    }

    #[test]
    fn legacy_process_action_unifies_to_staged_collect_save_process() {
        let job = SavedJob {
            identifiers: Default::default(),
            collect: crate::data::JobCollect {
                host: "prod".to_string(),
                diagnostic_type: "standard".to_string(),
                save_dir: None,
            },
            action: crate::data::JobAction::Process {
                output: crate::data::JobOutput::Stdout,
                selection: None,
            },
        };

        let unified = unify_saved_job(&job, host()).expect("unified job");
        assert!(matches!(unified.input(), Input::Collect { .. }));
        // Legacy actions always stage through a bundle; without a save_dir
        // the bundle is temporary (not retained)
        assert!(!unified.save().expect("save stage").is_retained());
        assert!(unified.process().is_some());
        assert_eq!(unified.execution_mode(), ExecutionMode::Staged);
    }

    #[test]
    fn legacy_upload_action_unifies_to_collect_save_send() {
        let job = SavedJob {
            identifiers: Default::default(),
            collect: crate::data::JobCollect {
                host: "prod".to_string(),
                diagnostic_type: "standard".to_string(),
                save_dir: Some(PathBuf::from("/tmp/keep")),
            },
            action: crate::data::JobAction::Upload {
                upload_id: "abc123".to_string(),
            },
        };

        let unified = unify_saved_job(&job, host()).expect("unified job");
        assert!(unified.save().expect("save stage").is_retained());
        assert!(unified.process().is_none());
        assert_eq!(unified.send().expect("send stage").upload_id, "abc123");
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
