// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Finite commands as typed operations. Each takes already-parsed input and
//! returns a [`CommandOutcome`] without parsing arguments, printing, or
//! prompting, so the CLI and other front ends share one implementation.

#[cfg(feature = "agent")]
pub mod agent;
pub mod collect;
pub mod host;
#[cfg(feature = "keystore")]
pub mod job;
pub mod keystore;
pub mod output;
pub mod process;
pub mod send;
#[cfg(feature = "setup")]
pub mod setup;

use crate::{
    cli_output::{
        BundleResult, CliOutcome, DiagnosticResult, FileCounts, HostResult, IncludedDiagnosticResult, IndexingFailure,
        JobInputResult, JobProcessResult, JobSaveResult, JobSendResult, ProcessResult, SavedJobResult, SendResult,
    },
    data::{HostRole, Job, KnownHost, Uri},
    job::{
        JobOutcome,
        model::{ExportTarget, Input},
        outcome::{ExecutionOutcome, StageStatus},
    },
    processor::{CollectionResult, DiagnosticOutcome},
};
use eyre::{Result, eyre};

pub fn collection_outcome(result: CollectionResult, send_destination: Option<String>) -> CliOutcome {
    CliOutcome::ArchiveCollected {
        path: result.path,
        files: FileCounts {
            successful: result.success,
            total: result.total,
        },
        duration_ms: Some(result.duration_ms),
        send_destination,
    }
}

pub fn format_execution_failure(outcome: &ExecutionOutcome) -> String {
    let mut failures = outcome
        .stages
        .iter()
        .filter_map(|stage| match &stage.status {
            StageStatus::Failed(error) => Some(format!("{:?} failed: {error}", stage.stage)),
            StageStatus::Blocked(reason) => Some(format!("{:?} blocked: {reason}", stage.stage)),
            StageStatus::Succeeded | StageStatus::Skipped(_) => None,
        })
        .collect::<Vec<_>>();
    failures.extend(outcome.child_failures());
    if failures.is_empty() {
        "Job did not complete successfully".to_string()
    } else {
        failures.join("\n")
    }
}

pub fn execution_process_result(outcome: &ExecutionOutcome) -> Option<ProcessResult> {
    let report = outcome.report.as_ref()?;
    let included = outcome
        .children
        .iter()
        .map(
            |child| match (child.diagnostic_outcome, child.report(), child.failure()) {
                (_, Some(_), Some(error)) => IncludedDiagnosticResult::Failed {
                    source: child.path.clone(),
                    error: error.to_string(),
                },
                (DiagnosticOutcome::Skipped(_), _, _) => IncludedDiagnosticResult::Skipped {
                    source: child.path.clone(),
                    product: Some(crate::processor::display_label(child.application(), child.platform())),
                    reason: child
                        .execution_error()
                        .unwrap_or("diagnostic processing skipped")
                        .to_string(),
                },
                (_, Some(report), None) => IncludedDiagnosticResult::Completed {
                    source: child.path.clone(),
                    diagnostic: DiagnosticResult {
                        id: report.diagnostic.metadata.id.clone(),
                        product: report.diagnostic.display_label(),
                        documents: report.diagnostic.docs.created,
                        documents_failed: report.diagnostic.docs.errors,
                        outcome: report.outcome(),
                        indexing_failures: report
                            .rejected_indices()
                            .into_iter()
                            .map(|(index, documents_failed)| IndexingFailure {
                                index,
                                documents_failed,
                                category: "document_rejected",
                            })
                            .collect(),
                        duration_ms: child.runtime.unwrap_or_default(),
                        source: child.path.clone(),
                        output: String::new(),
                        kibana_url: report.diagnostic.kibana_link.clone(),
                    },
                },
                (_, None, _) => IncludedDiagnosticResult::Failed {
                    source: child.path.clone(),
                    error: child
                        .execution_error()
                        .unwrap_or("included diagnostic processing failed")
                        .to_string(),
                },
            },
        )
        .collect();

    Some(ProcessResult {
        diagnostic: DiagnosticResult {
            id: report.diagnostic.metadata.id.clone(),
            product: report.diagnostic.display_label(),
            documents: report.diagnostic.docs.created,
            documents_failed: report.diagnostic.docs.errors,
            outcome: report.outcome(),
            indexing_failures: report
                .rejected_indices()
                .into_iter()
                .map(|(index, documents_failed)| IndexingFailure {
                    index,
                    documents_failed,
                    category: "document_rejected",
                })
                .collect(),
            duration_ms: report.diagnostic.processing_duration,
            source: "primary".to_string(),
            output: String::new(),
            kibana_url: report.diagnostic.kibana_link.clone(),
        },
        included,
    })
}

pub fn job_outcome(name: String, outcome: JobOutcome) -> CliOutcome {
    CliOutcome::JobCompleted {
        job: name,
        save: outcome
            .bundle_path
            .filter(|_| outcome.bundle_retained && outcome.bundle_created)
            .map(|path| BundleResult {
                path: path.display().to_string(),
            }),
        process: outcome.execution.as_ref().and_then(execution_process_result),
        send: outcome.send_slug.map(|slug| SendResult {
            destination: format!("https://upload.elastic.co/g/{slug}"),
        }),
    }
}

pub fn host_result(name: String, host: &KnownHost) -> HostResult {
    HostResult {
        name,
        app: host.app().map(|app| app.key().to_string()),
        roles: host.roles().iter().map(ToString::to_string).collect(),
        target: host.transport_display(),
        secret_reference: host.secret_reference().map(str::to_string),
    }
}

pub fn saved_job_result(name: String, job: &Job) -> SavedJobResult {
    let input = match job.input() {
        Input::Collect {
            host, diagnostic_type, ..
        } => JobInputResult::Collect {
            host: host.clone(),
            diagnostic_type: diagnostic_type.clone(),
        },
        Input::CollectBinding {
            binding,
            diagnostic_type,
            ..
        } => JobInputResult::Collect {
            host: format!("binding:{}", binding.as_str()),
            diagnostic_type: diagnostic_type.clone(),
        },
        Input::Load { uri } => JobInputResult::Load {
            source: uri.to_string(),
        },
        Input::LoadBinding { binding } => JobInputResult::Load {
            source: format!("binding:{}", binding.as_str()),
        },
    };
    let save = job.save().map(|save| JobSaveResult {
        directory: save.dir.as_ref().map(|path| path.display().to_string()),
    });
    let process = job.process().map(|process| JobProcessResult {
        export: match &process.export {
            ExportTarget::KnownHost { name } => format!("host:{name}"),
            ExportTarget::Environment => "environment".to_string(),
            ExportTarget::File { path } => path.display().to_string(),
            ExportTarget::Directory { output_dir } => output_dir.display().to_string(),
            ExportTarget::Stdout => "-".to_string(),
            ExportTarget::Binding { binding } => format!("binding:{}", binding.as_str()),
        },
    });
    let send = job.send().map(|send| JobSendResult {
        upload_id: send.upload_id.clone(),
    });
    SavedJobResult {
        name,
        input,
        save,
        process,
        send,
    }
}

#[cfg(feature = "setup")]
pub fn setup_outcome(targets: Vec<String>, report: crate::setup::SetupReport) -> CliOutcome {
    let outcome = if report.is_complete() { "complete" } else { "partial" }.to_string();
    CliOutcome::SetupCompleted {
        targets,
        outcome,
        failed_indices: report.failed_indices,
        warnings: report.warnings.into_iter().chain(report.notices).collect(),
    }
}

pub fn ensure_host_role(host: &KnownHost, role: HostRole, context: &str) -> Result<()> {
    if host.has_role(role.clone()) {
        Ok(())
    } else {
        Err(eyre!(
            "Host role validation failed for {context}: required role '{}' not present",
            role
        ))
    }
}

pub fn ensure_uri_role(uri: &Uri, role: HostRole, context: &str) -> Result<()> {
    match uri {
        Uri::KnownHost(host) | Uri::ElasticCloudAdmin(host) | Uri::ElasticGovCloudAdmin(host) => {
            ensure_host_role(host, role, context)
        }
        _ => Ok(()),
    }
}

/// Renders an output target without credentials, query, or fragment.
pub fn safe_output_display(uri: &Uri) -> String {
    match uri {
        Uri::KnownHost(host)
        | Uri::ElasticCloud(host)
        | Uri::ElasticCloudAdmin(host)
        | Uri::ElasticGovCloudAdmin(host) => host
            .concrete_url()
            .map(|url| safe_output_display(&Uri::Url(url.clone())))
            .unwrap_or_else(|| host.transport_display()),
        Uri::Url(url) | Uri::ServiceLink(url) | Uri::ServiceLinkNoAuth(url) => {
            let mut url = url.clone();
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        }
        _ => uri.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data::{Application, KnownHostBuilder, Platform},
        job::{
            context::ExecutionIdentity,
            outcome::{ChildExecutionOutcome, Stage, StageOutcome},
        },
    };
    use url::Url;

    #[test]
    fn output_display_redacts_direct_and_resolved_url_credentials() {
        let url = Url::parse("https://user:password@example.test:9200/path?token=secret#fragment").unwrap();
        let direct = safe_output_display(&Uri::Url(url.clone()));
        let host = KnownHostBuilder::new(url)
            .application(Application::Elasticsearch)
            .build()
            .unwrap();
        let resolved = safe_output_display(&Uri::KnownHost(host));
        assert_eq!(direct, "https://example.test:9200/path");
        assert_eq!(resolved, direct);
    }

    #[cfg(feature = "setup")]
    #[test]
    fn setup_outcome_reports_partial_mapping_updates() {
        let outcome = setup_outcome(
            vec!["elasticsearch".into()],
            crate::setup::SetupReport {
                failed_indices: vec!["metrics-test-esdiag".into()],
                warnings: vec!["Raise index.mapping.total_fields.limit".into()],
                notices: Vec::new(),
            },
        );
        let value = serde_json::to_value(outcome).unwrap();
        assert_eq!(value["outcome"], "partial");
        assert_eq!(value["failed_indices"][0], "metrics-test-esdiag");
    }

    #[test]
    fn included_report_failure_is_not_rendered_as_completed() {
        let child_execution = ExecutionOutcome {
            identity: ExecutionIdentity::new(2, "test"),
            stages: vec![StageOutcome {
                stage: Stage::Process,
                status: StageStatus::Failed("report write rejected".to_string()),
            }],
            collection: None,
            report: None,
            children: Vec::new(),
            retained_bundle: None,
            send: None,
        };
        let child = ChildExecutionOutcome {
            path: "child-es".to_string(),
            execution: Box::new(child_execution),
            diagnostic_outcome: DiagnosticOutcome::Failed,
            application: Some(Application::Elasticsearch),
            platform: Platform::ECK,
            runtime: Some(1),
        };
        let mut outcome = ExecutionOutcome::new(ExecutionIdentity::new(1, "test"));
        outcome.children.push(child);
        assert!(outcome.has_child_failures());
        assert_eq!(
            format_execution_failure(&outcome),
            "Included diagnostic child-es failed: report write rejected"
        );
    }
}
