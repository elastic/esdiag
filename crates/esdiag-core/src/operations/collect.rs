// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{collection_outcome, ensure_host_role, format_execution_failure, process::sources_application_key};
use crate::{
    cli_output::CommandOutcome,
    data::{HostRole, Uri, collect_application},
    job::{
        context::ExecutionContext,
        executor::execute_with_context,
        model::{BindingKey, Input, Job, SaveTarget, SendTarget},
    },
    processor::{Identifiers, default_collect_archive_name},
    receiver::Receiver,
};
use eyre::{Result, eyre};

#[derive(Debug, Clone, Default)]
pub struct CollectInput {
    /// Saved known host name or Elastic CLI context reference.
    pub host: String,
    /// Local directory that receives the archive.
    pub output: String,
    pub diagnostic_type: String,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    /// Path to a `sources.yml` override for the host's product.
    pub sources: Option<String>,
    pub account: Option<String>,
    pub case: Option<String>,
    pub opportunity: Option<String>,
    pub user: Option<String>,
    /// Elastic Upload Service ID or URL to send the archive to after collection.
    pub send_id: Option<String>,
    /// Save the effective invocation as this named job before collecting.
    #[cfg(feature = "keystore")]
    pub save_job: Option<String>,
}

/// Collects a diagnostic archive from a known host.
pub async fn collect(input: CollectInput) -> Result<CommandOutcome> {
    let CollectInput {
        host,
        output,
        diagnostic_type,
        include,
        exclude,
        sources,
        account,
        case,
        opportunity,
        user,
        send_id,
        #[cfg(feature = "keystore")]
        save_job,
    } = input;
    #[cfg(feature = "keystore")]
    if let Some(name) = save_job.as_deref() {
        let identifiers = Identifiers::new(account.clone(), case.clone(), None, opportunity.clone(), user.clone());
        let job = super::job::derive_collect_job(&host, &output, &diagnostic_type, send_id.as_deref(), identifiers)?;
        super::job::job_save(name, job)?;
    }
    let known_host = Uri::try_from(host.as_str())?;
    let output = Uri::try_from(output)?;
    match known_host {
        Uri::KnownHost(host) | Uri::ElasticCloudAdmin(host) | Uri::ElasticGovCloudAdmin(host) => {
            ensure_host_role(&host, HostRole::Collect, "collect")?;
            let application = collect_application(host.app())?;
            if let Some(sources) = sources {
                crate::processor::init_sources(sources_application_key(application)?, sources)?;
            }
            tracing::info!("Collecting diagnostic from {host}");
            tracing::info!("Saving diagnostic to {output}");
            let output_dir = match output {
                Uri::Directory(path) | Uri::File(path) => path,
                _ => {
                    return Err(eyre!("Collect output must be a local directory path"));
                }
            };
            let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
            let filename = format!("{}.zip", default_collect_archive_name(application, &timestamp));
            let identifiers = Identifiers::new(account, case, Some(filename), opportunity, user);
            let binding = BindingKey::try_new("cli-collect-input")?;
            let mut context = ExecutionContext::default();
            context
                .inputs
                .bind_receiver(binding.clone(), Receiver::try_from(host)?, Some(application));
            let job = Job::try_new(
                identifiers,
                Input::CollectBinding {
                    binding,
                    diagnostic_type,
                    include,
                    exclude,
                },
                Some(SaveTarget::retained(output_dir)),
                None,
                send_id.map(|upload_id| SendTarget { upload_id }),
            )?;
            let outcome = execute_with_context(job, context).await;
            if !outcome.succeeded() {
                return Err(eyre!("{}", format_execution_failure(&outcome)));
            }
            let result = outcome
                .collection
                .ok_or_else(|| eyre!("Collection completed without a collection result"))?;
            let send_destination = outcome
                .send
                .as_ref()
                .map(|send| format!("https://upload.elastic.co/g/{}", send.slug));
            Ok(CommandOutcome::Completed(collection_outcome(result, send_destination)))
        }
        Uri::ElasticCloud(_) => Err(eyre!("Elastic Cloud API collection not yet implemented")),
        _ => Err(eyre!("Collect requires a known host")),
    }
}
