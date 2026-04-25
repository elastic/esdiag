// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Shared job runner for executing saved diagnostic jobs.
//! Used by both the CLI (`esdiag job run`) and the web server.

use crate::{
    data::{Job, JobAction, JobOutput, JobProcessSelection, KnownHost, Uri, load_saved_jobs, save_saved_jobs},
    exporter::Exporter,
    processor::{
        Collector, Identifiers, Processor,
        api::{ApiResolver, ProcessSelection},
    },
    receiver::Receiver,
    uploader,
};
use eyre::{Result, eyre};
use std::{io::IsTerminal, path::PathBuf, sync::Arc};

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

pub fn save_job(name: &str, job: Job) -> Result<()> {
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

pub async fn run_job(job: Job, host: KnownHost) -> Result<()> {
    let host_url = host.get_url().to_string();
    tracing::info!("Running saved job against {host_url}");

    match &job.action {
        JobAction::Collect { output_dir } => {
            let archive_path = collect_job_archive(&job, host, output_dir.clone(), &host_url).await?;
            tracing::info!("Collected archive: {}", archive_path.display());
            Ok(())
        }
        JobAction::Upload { upload_id } => {
            let (output_dir, _cleanup) = collect_output_dir(&job)?;
            let archive_path = collect_job_archive(&job, host, output_dir, &host_url).await?;
            let response = uploader::upload_file(&archive_path, upload_id, uploader::DEFAULT_UPLOAD_API_URL).await?;
            tracing::info!("Forwarded archive to https://upload.elastic.co/g/{}", response.slug);
            if job.collect.save_dir.is_some() {
                tracing::info!("Retained collected archive: {}", archive_path.display());
            }
            Ok(())
        }
        JobAction::Process { output, selection } => {
            let (output_dir, _cleanup) = collect_output_dir(&job)?;
            let archive_path = collect_job_archive(&job, host, output_dir, &host_url).await?;
            tracing::info!("Collected archive: {}", archive_path.display());
            process_archive(
                archive_path.clone(),
                output,
                selection.as_ref(),
                job.identifiers.clone(),
            )
            .await?;
            if job.collect.save_dir.is_some() {
                tracing::info!("Retained collected archive: {}", archive_path.display());
            }
            Ok(())
        }
    }
}

async fn collect_job_archive(job: &Job, host: KnownHost, output_dir: PathBuf, host_url: &str) -> Result<PathBuf> {
    tracing::info!("Collecting diagnostic from {host_url}");
    let product = host.app().clone();
    let diagnostic_type = job.collect.diagnostic_type.clone();
    let receiver = Receiver::try_from(host)?;
    let collect_exporter = Exporter::for_collect_archive(output_dir)?;
    let collector = Collector::try_new(
        receiver,
        collect_exporter,
        product,
        diagnostic_type,
        None,
        None,
        job.identifiers.clone(),
    )
    .await?;
    let result = collector.collect().await?;
    Ok(PathBuf::from(&result.path))
}

fn collect_output_dir(job: &Job) -> Result<(PathBuf, Option<TempDirCleanup>)> {
    if let Some(save_dir) = &job.collect.save_dir {
        Ok((save_dir.clone(), None))
    } else {
        let temp_dir = std::env::temp_dir().join(format!("esdiag-job-{}", uuid::Uuid::new_v4().as_u64_pair().0));
        std::fs::create_dir_all(&temp_dir)?;
        Ok((temp_dir.clone(), Some(TempDirCleanup(temp_dir))))
    }
}

async fn process_archive(
    archive_path: PathBuf,
    output: &JobOutput,
    selection: Option<&JobProcessSelection>,
    identifiers: Identifiers,
) -> Result<()> {
    let exporter = resolve_exporter(output)?;
    let receiver = Arc::new(Receiver::try_from(Uri::File(archive_path))?);
    let exporter = Arc::new(exporter);
    let process_selection = explicit_process_selection(selection)?;
    let processor = Processor::try_new_with_selection(receiver, exporter, identifiers, process_selection).await?;
    let processor = processor.start().await.map_err(|failed| eyre!("{}", failed))?;
    match processor.process().await {
        Ok(completed) => {
            tracing::info!("Processing complete in {:.3}s", completed.state.runtime as f64 / 1000.0);
            Ok(())
        }
        Err(failed) => Err(eyre!("{}", failed)),
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

fn resolve_exporter(output: &JobOutput) -> Result<Exporter> {
    match output {
        JobOutput::KnownHost { name } => Exporter::try_from(Uri::try_from(name.clone())?),
        JobOutput::File { path } => Exporter::try_from(Uri::try_from(path.display().to_string())?),
        JobOutput::Directory { output_dir } => Exporter::try_from(Uri::try_from(output_dir.display().to_string())?),
        JobOutput::Stdout => Exporter::try_from(Uri::Stream),
    }
}

struct TempDirCleanup(PathBuf);

impl Drop for TempDirCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::{explicit_process_selection, validate_saved_job_name};
    use crate::data::{Job, JobProcessSelection};
    use std::path::PathBuf;

    #[test]
    fn job_builder_creates_collect_job() {
        let job = Job {
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
        assert!(selection.selected.iter().any(|item| item == "node"));
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
                .expect_err("slash should be rejected")
                .to_string(),
            "Job name contains unsupported path characters"
        );
    }
}
