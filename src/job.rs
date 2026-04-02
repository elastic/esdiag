// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Shared job runner for executing saved diagnostic jobs.
//! Used by both the CLI (`esdiag job run`) and the web server.

use crate::{
    data::{
        CollectMode, CollectSource, KnownHost, ProcessMode, SendMode, Uri, Workflow,
        load_saved_jobs, save_saved_jobs,
    },
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
        let collect_target = &job.workflow.collect.known_host;
        let stale = !collect_target.is_empty() && !hosts.contains_key(collect_target);
        let collect_display = if stale && use_color {
            format!("\x1b[31m{collect_target}\x1b[0m")
        } else {
            collect_target.clone()
        };

        let processing = if job.workflow.process.enabled {
            &job.workflow.process.diagnostic_type
        } else {
            "skipped"
        };

        let send_target = match job.workflow.send.mode {
            SendMode::Remote => job.workflow.send.remote_target.clone(),
            SendMode::Local => {
                if job.workflow.send.local_target == "directory" {
                    format!("dir:{}", job.workflow.send.local_directory)
                } else {
                    job.workflow.send.local_target.clone()
                }
            }
        };

        println!(
            "{:<24} {:<24} {:<16} {}",
            name, collect_display, processing, send_target
        );
    }

    Ok(())
}

pub async fn handle_job_run(name: &str) -> Result<()> {
    let jobs = load_saved_jobs()?;
    let job = jobs
        .get(name)
        .ok_or_else(|| eyre!("Saved job '{}' not found", name))?;

    let host_name = &job.workflow.collect.known_host;
    if host_name.is_empty() {
        return Err(eyre!(
            "Saved job '{}' has no collection host configured",
            name
        ));
    }

    let host = KnownHost::get_known(host_name).ok_or_else(|| {
        eyre!(
            "Host '{}' referenced by job '{}' not found in hosts.yml",
            host_name,
            name
        )
    })?;

    tracing::info!("Running saved job '{name}'");
    run_saved_job(&job.workflow, job.identifiers.clone(), host).await?;
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

pub async fn run_saved_job(
    workflow: &Workflow,
    identifiers: Identifiers,
    host: KnownHost,
) -> Result<()> {
    validate_saved_job_workflow(workflow)?;
    let host_url = host.get_url().to_string();
    tracing::info!("Running saved job against {host_url}");

    let need_collect = workflow.collect.mode == CollectMode::Collect;
    let need_process = workflow.process.enabled && workflow.process.mode == ProcessMode::Process;

    if need_collect && need_process {
        // Collect → Process → Send
        let (output_dir, _cleanup) = if workflow.collect.save {
            (collect_output_dir(workflow)?, None)
        } else {
            let temp_dir = std::env::temp_dir().join(format!(
                "esdiag-job-{}",
                uuid::Uuid::new_v4().as_u64_pair().0
            ));
            std::fs::create_dir_all(&temp_dir)?;
            (temp_dir.clone(), Some(TempDirCleanup(temp_dir)))
        };

        tracing::info!("Collecting diagnostic from {host_url}");
        let product = host.app().clone();
        let diagnostic_type = workflow.collect.diagnostic_type.clone();
        let receiver = Receiver::try_from(host)?;
        let collect_exporter = Exporter::for_collect_archive(output_dir)?;
        let collector = Collector::try_new(
            receiver,
            collect_exporter,
            product,
            diagnostic_type,
            None,
            None,
            identifiers.clone(),
        )
        .await?;
        let result = collector.collect().await?;
        let archive_path = PathBuf::from(result.path);
        tracing::info!("Collected archive: {}", archive_path.display());

        // Process the collected archive
        let exporter = resolve_exporter(workflow)?;
        let receiver = Arc::new(Receiver::try_from(Uri::File(archive_path.clone()))?);
        let exporter = Arc::new(exporter);
        let process_selection = explicit_process_selection(workflow)?;
        let processor =
            Processor::try_new_with_selection(receiver, exporter, identifiers, process_selection)
                .await?;
        let processor = processor
            .start()
            .await
            .map_err(|failed| eyre!("{}", failed))?;
        match processor.process().await {
            Ok(completed) => {
                tracing::info!(
                    "Processing complete in {:.3}s",
                    completed.state.runtime as f64 / 1000.0
                );
                if workflow.collect.save {
                    tracing::info!("Retained collected archive: {}", archive_path.display());
                }
                Ok(())
            }
            Err(failed) => Err(eyre!("{}", failed)),
        }
    } else if need_collect {
        run_saved_job_collect_only(workflow, identifiers, host, &host_url).await
    } else {
        Err(eyre!("Saved job has no valid execution path"))
    }
}

fn validate_saved_job_workflow(workflow: &Workflow) -> Result<()> {
    if workflow.collect.mode != CollectMode::Collect {
        return Err(eyre!("Saved jobs require collect mode"));
    }
    if workflow.collect.source != CollectSource::KnownHost {
        return Err(eyre!("Saved jobs require a known-host collection source"));
    }
    Ok(())
}

async fn run_saved_job_collect_only(
    workflow: &Workflow,
    identifiers: Identifiers,
    host: KnownHost,
    host_url: &str,
) -> Result<()> {
    let use_temp_output = workflow.process.mode == ProcessMode::Forward
        && workflow.send.mode == SendMode::Remote
        && !workflow.collect.save;
    let (output_dir, _cleanup) = if use_temp_output {
        let temp_dir = std::env::temp_dir().join(format!(
            "esdiag-job-forward-{}",
            uuid::Uuid::new_v4().as_u64_pair().0
        ));
        std::fs::create_dir_all(&temp_dir)?;
        (temp_dir.clone(), Some(TempDirCleanup(temp_dir)))
    } else {
        (collect_output_dir(workflow)?, None)
    };
    tracing::info!("Collecting diagnostic from {host_url}");
    let product = host.app().clone();
    let diagnostic_type = workflow.collect.diagnostic_type.clone();
    let receiver = Receiver::try_from(host)?;
    let collect_exporter = Exporter::for_collect_archive(output_dir)?;
    let collector = Collector::try_new(
        receiver,
        collect_exporter,
        product,
        diagnostic_type,
        None,
        None,
        identifiers,
    )
    .await?;
    let result = collector.collect().await?;
    let archive_path = PathBuf::from(&result.path);

    if workflow.process.mode == ProcessMode::Forward && workflow.send.mode == SendMode::Remote {
        let target = workflow.send.remote_target.trim();
        if target.is_empty() {
            return Err(eyre!(
                "Remote forward requires an Elastic Upload Service upload id or URL"
            ));
        }

        let response =
            uploader::upload_file(&archive_path, target, uploader::DEFAULT_UPLOAD_API_URL).await?;
        tracing::info!(
            "Forwarded archive to https://upload.elastic.co/g/{}",
            response.slug
        );
        if workflow.collect.save {
            tracing::info!("Retained collected archive: {}", archive_path.display());
        }
        return Ok(());
    }

    tracing::info!("Collected archive: {}", archive_path.display());
    Ok(())
}

fn collect_output_dir(workflow: &Workflow) -> Result<PathBuf> {
    if workflow.collect.save_dir.is_empty() {
        Ok(std::env::current_dir()?)
    } else {
        Ok(PathBuf::from(&workflow.collect.save_dir))
    }
}

fn explicit_process_selection(workflow: &Workflow) -> Result<Option<ProcessSelection>> {
    let has_explicit_choice = !workflow.process.selected.trim().is_empty()
        || workflow.process.product != "elasticsearch"
        || workflow.process.diagnostic_type != "standard";
    if !has_explicit_choice {
        return Ok(None);
    }

    let selected: Vec<String> = workflow
        .process
        .selected
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect();
    let selected = ApiResolver::resolve_processing_selection(
        &workflow.process.product,
        &workflow.process.diagnostic_type,
        &selected,
    )?;

    Ok(Some(ProcessSelection {
        product: workflow.process.product.clone(),
        diagnostic_type: workflow.process.diagnostic_type.clone(),
        selected,
    }))
}

fn resolve_exporter(workflow: &Workflow) -> Result<Exporter> {
    match workflow.send.mode {
        SendMode::Remote => {
            let target = workflow.send.remote_target.trim();
            if target.is_empty() {
                return Err(eyre!("Remote send target is empty"));
            }
            Exporter::try_from(Uri::try_from(target.to_string())?)
        }
        SendMode::Local => {
            let target = workflow.send.local_target.trim();
            if target == "directory" {
                let directory = workflow.send.local_directory.trim();
                if directory.is_empty() {
                    return Err(eyre!("Local directory output requires a directory path"));
                }
                Exporter::try_from(Uri::try_from(directory.to_string())?)
            } else if target.is_empty() {
                Err(eyre!("Local send requires a target"))
            } else {
                Exporter::try_from(Uri::try_from(target.to_string())?)
            }
        }
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
    use super::{collect_output_dir, explicit_process_selection, validate_saved_job_workflow};
    use crate::data::{CollectSource, Workflow};
    use std::path::PathBuf;

    #[test]
    fn collect_output_dir_prefers_saved_directory() {
        let mut workflow = Workflow::default();
        workflow.collect.save_dir = "/tmp/esdiag-saved-jobs".to_string();

        assert_eq!(
            collect_output_dir(&workflow).expect("save dir"),
            PathBuf::from("/tmp/esdiag-saved-jobs")
        );
    }

    #[test]
    fn explicit_process_selection_uses_saved_workflow_values() {
        let mut workflow = Workflow::default();
        workflow.process.product = "logstash".to_string();
        workflow.process.diagnostic_type = "standard".to_string();
        workflow.process.selected = "node".to_string();

        let selection = explicit_process_selection(&workflow)
            .expect("selection")
            .expect("saved selection");

        assert_eq!(selection.product, "logstash");
        assert!(selection.selected.iter().any(|item| item == "node"));
    }

    #[test]
    fn validate_saved_job_workflow_rejects_non_known_host_sources() {
        let mut workflow = Workflow::default();
        workflow.collect.source = CollectSource::UploadFile;

        assert_eq!(
            validate_saved_job_workflow(&workflow)
                .expect_err("upload-file saved jobs should be rejected")
                .to_string(),
            "Saved jobs require a known-host collection source"
        );
    }
}
