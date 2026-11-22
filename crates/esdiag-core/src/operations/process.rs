// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{ensure_uri_role, execution_process_result, format_execution_failure, safe_output_display};
#[cfg(feature = "agent")]
use crate::agent::builder::AgentProgress;
#[cfg(feature = "agent")]
use crate::data::OutputDeployment;
use crate::{
    cli_output::{CliOutcome, CommandOutcome, DiagnosticResult},
    data::{Application, ApplicationConfig, HostReference, HostRole, Uri},
    job::{
        FailedStage, JobExecutionFailure, JobOutcome,
        context::ExecutionContext,
        executor::execute_with_context,
        model::{BindingKey, ExportTarget, Input, Job, Process},
    },
    processor::Identifiers,
    receiver::Receiver,
};
use eyre::{Result, eyre};

#[derive(Debug, Clone, Default)]
pub struct ProcessInput {
    /// Archive path, directory, known host, or URL to read the diagnostic from.
    pub input: String,
    /// Output target; `None` uses the configured default output.
    pub output: Option<String>,
    /// Prompt for Agent Builder about the processed diagnostic.
    #[cfg(feature = "agent")]
    pub ask: Option<String>,
    pub account: Option<String>,
    pub case: Option<String>,
    pub opportunity: Option<String>,
    pub user: Option<String>,
    /// Path to a `sources.yml` override for the detected product.
    pub sources: Option<String>,
    /// Force scrubbed archive mode; `None` detects it from the file name.
    pub scrubbed: Option<bool>,
    /// Save the effective invocation as this named job before processing.
    #[cfg(feature = "keystore")]
    pub save_job: Option<String>,
}

impl ProcessInput {
    /// Whether processed documents are written to stdout, which yields
    /// [`CommandOutcome::Streamed`].
    pub fn writes_stdout(&self) -> bool {
        matches!(self.output.as_deref(), Some("-" | "stdio://stdout"))
    }
}

/// Processes one diagnostic and exports its documents.
pub async fn process(input: ProcessInput) -> Result<CommandOutcome> {
    #[cfg(feature = "agent")]
    {
        process_with_agent_progress(input, |_, _| {}).await
    }
    #[cfg(not(feature = "agent"))]
    {
        Ok(match execute(input).await?.diagnostic {
            Some(diagnostic) => CommandOutcome::Completed(CliOutcome::DiagnosticProcessed { diagnostic }),
            None => CommandOutcome::Streamed,
        })
    }
}

/// Like [`process`], reporting Agent Builder progress for `ask` to `progress`.
#[cfg(feature = "agent")]
pub async fn process_with_agent_progress<F>(input: ProcessInput, progress: F) -> Result<CommandOutcome>
where
    F: FnMut(&str, AgentProgress),
{
    let Processed { diagnostic, ask } = execute(input).await?;
    let Some(diagnostic) = diagnostic else {
        return Ok(CommandOutcome::Streamed);
    };
    if let Some((prompt, deployment)) = ask {
        return super::agent::agent_ask_with_deployment(
            super::agent::AgentAskInput {
                prompt: super::agent::process_ask_prompt(&diagnostic.id, &prompt),
                agent_id: super::agent::DEFAULT_AGENT_BUILDER_AGENT.to_string(),
                conversation: None,
            },
            deployment,
            progress,
        )
        .await;
    }
    Ok(CommandOutcome::Completed(CliOutcome::DiagnosticProcessed {
        diagnostic,
    }))
}

struct Processed {
    /// `None` when documents were streamed to stdout.
    diagnostic: Option<DiagnosticResult>,
    #[cfg(feature = "agent")]
    ask: Option<(String, OutputDeployment)>,
}

async fn execute(input: ProcessInput) -> Result<Processed> {
    let ProcessInput {
        input,
        output,
        #[cfg(feature = "agent")]
        ask,
        account,
        case,
        opportunity,
        user,
        sources,
        scrubbed,
        #[cfg(feature = "keystore")]
        save_job,
    } = input;
    let has_explicit_output = output.is_some();
    #[cfg(feature = "agent")]
    if ask.is_some() && output.as_deref() == Some("-") {
        return Err(eyre!(ProcessAskStdoutConflict));
    }
    let input_uri = Uri::try_from(input.as_str())?;
    #[cfg(feature = "agent")]
    let (output_uri, ask_deployment) = if ask.is_some() {
        let deployment = OutputDeployment::resolve(output.as_deref(), true)?;
        let uri = Uri::try_from(deployment.elasticsearch.clone())?;
        (uri, Some(deployment))
    } else {
        (Uri::try_from(output.clone())?, None)
    };
    #[cfg(not(feature = "agent"))]
    let output_uri = Uri::try_from(output.clone())?;
    if let Uri::KnownHost(host) = &output_uri
        && host.app().is_some_and(|app| app != Application::Elasticsearch)
    {
        return Err(eyre!(
            "Output host '{}' must be an Elasticsearch host",
            output.as_deref().unwrap_or_default()
        ));
    }
    #[cfg(feature = "keystore")]
    if let Some(name) = save_job.as_deref() {
        let identifiers = Identifiers::new(account.clone(), case.clone(), None, opportunity.clone(), user.clone());
        let job = super::job::derive_process_job(&input, output.as_deref(), identifiers)?;
        super::job::job_save(name, job)?;
    }
    let output_display = match output.as_deref() {
        Some(name) if matches!(HostReference::parse(name), Ok(Some(_))) => name.to_string(),
        None if std::env::var_os("ESDIAG_OUTPUT_URL").is_none() => ApplicationConfig::load()?
            .output
            .default
            .unwrap_or_else(|| safe_output_display(&output_uri)),
        _ => safe_output_display(&output_uri),
    };
    let stdout_owned = matches!(output_uri, Uri::Stream);
    if has_explicit_output {
        ensure_uri_role(&output_uri, HostRole::Send, "process output")?;
    }
    #[cfg(feature = "agent")]
    if ask.is_some() && stdout_owned {
        return Err(eyre!(ProcessAskStdoutConflict));
    }
    tracing::info!("input: {}", input_uri);

    let receiver = Receiver::try_from_with_scrub(input_uri.clone(), scrubbed, None)?;
    if let Some(sources) = sources {
        let application = detect_sources_application(&input_uri, &receiver).await?;
        crate::processor::init_sources(sources_application_key(application)?, sources)?;
    }
    let identifiers = Identifiers::new(account, case, receiver.filename(), opportunity, user);
    let mut context = ExecutionContext::default();
    let binding = BindingKey::try_new("cli-process-input")?;
    let live_application = match &input_uri {
        Uri::KnownHost(host) | Uri::ElasticCloudAdmin(host) | Uri::ElasticGovCloudAdmin(host) => Some(host.app()),
        _ => None,
    };
    match &input_uri {
        Uri::File(path) => {
            context
                .inputs
                .bind_bundle(binding.clone(), receiver, path.clone(), live_application.flatten())
        }
        _ => context
            .inputs
            .bind_receiver(binding.clone(), receiver, live_application.flatten()),
    };
    let job_input = if live_application.is_some() {
        Input::CollectBinding {
            binding,
            diagnostic_type: "standard".to_string(),
            include: None,
            exclude: None,
        }
    } else {
        Input::LoadBinding { binding }
    };
    let export_target = match &output_uri {
        Uri::File(path) => ExportTarget::File { path: path.clone() },
        Uri::Directory(output_dir) => ExportTarget::Directory {
            output_dir: output_dir.clone(),
        },
        Uri::Stream => ExportTarget::Stdout,
        _ => {
            let binding = BindingKey::try_new("cli-process-output")?;
            context.bind_document_exporter(
                binding.clone(),
                crate::exporter::DocumentExporter::try_from(output_uri)?,
            );
            ExportTarget::Binding { binding }
        }
    };
    let job = Job::try_new(
        identifiers,
        job_input,
        None,
        Some(Process {
            selection: None,
            export: export_target,
        }),
        None,
    )?;
    let mut outcome = execute_with_context(job, context).await;
    let process = execution_process_result(&outcome);
    if outcome.failed() {
        let source = outcome.failure_error(format_execution_failure(&outcome));
        return Err(JobExecutionFailure::new(
            FailedStage::Process,
            JobOutcome {
                processed: outcome.report.is_some(),
                execution: Some(outcome),
                ..JobOutcome::default()
            },
            source,
        )
        .into());
    }
    let diagnostic = if stdout_owned {
        None
    } else {
        let mut diagnostic = process
            .map(|result| result.diagnostic)
            .ok_or_else(|| eyre!("Process completed without a diagnostic report"))?;
        diagnostic.output = output_display;
        Some(diagnostic)
    };
    Ok(Processed {
        diagnostic,
        #[cfg(feature = "agent")]
        ask: ask.zip(ask_deployment),
    })
}

pub(crate) fn sources_application_key(application: Application) -> Result<&'static str> {
    crate::processor::diagnostic::data_source::source_application_key(application).map_err(|_| {
        eyre!(
            "--sources is only supported for Elasticsearch, Kibana, and Logstash inputs, got {}",
            application
        )
    })
}

async fn detect_sources_application(input_uri: &Uri, receiver: &Receiver) -> Result<Application> {
    match input_uri {
        Uri::KnownHost(host) | Uri::ElasticCloudAdmin(host) | Uri::ElasticGovCloudAdmin(host) => host
            .app()
            .ok_or_else(|| eyre!("--sources is only supported for application diagnostics")),
        _ => receiver
            .try_get_manifest_from_files()
            .await?
            .application()
            .ok_or_else(|| eyre!("--sources is only supported for application diagnostics")),
    }
}

#[cfg(feature = "agent")]
#[derive(Debug)]
pub struct ProcessAskStdoutConflict;

#[cfg(feature = "agent")]
impl std::fmt::Display for ProcessAskStdoutConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "--ask cannot be used when process output is '-' because processed documents own stdout"
        )
    }
}

#[cfg(feature = "agent")]
impl std::error::Error for ProcessAskStdoutConflict {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_stdout_only_for_the_stream_target() {
        for (output, expected) in [
            (Some("-"), true),
            (Some("stdio://stdout"), true),
            (Some("/tmp/out.ndjson"), false),
            (None, false),
        ] {
            let input = ProcessInput {
                input: "diag.zip".to_string(),
                output: output.map(str::to_string),
                ..ProcessInput::default()
            };
            assert_eq!(input.writes_stdout(), expected, "{output:?}");
        }
    }
}
