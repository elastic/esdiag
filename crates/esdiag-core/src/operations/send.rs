// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{
    cli_output::{CliOutcome, CommandOutcome},
    elastic_upload_service,
};
use eyre::Result;

#[derive(Debug, Clone)]
pub struct SendInput {
    /// Archive file name or path; relative paths, including bare names, resolve against the current working directory.
    pub file_name: String,
    pub upload_id: String,
    pub api_url: String,
}

/// Uploads a diagnostic archive to the Elastic Upload Service.
pub async fn send(input: SendInput) -> Result<CommandOutcome> {
    let file_path = elastic_upload_service::default_upload_path(&input.file_name);
    tracing::info!(
        "Sending raw diagnostic archive {} to Elastic Upload Service target {}",
        file_path.display(),
        input.upload_id
    );
    let response = elastic_upload_service::BundleSender::new(input.api_url.clone())
        .send(&file_path, &input.upload_id)
        .await?;
    tracing::info!("Elastic Upload Service send complete for slug {}", response.slug);
    Ok(CommandOutcome::Completed(CliOutcome::DiagnosticSent {
        destination: format!("{}/g/{}", input.api_url.trim_end_matches('/'), response.slug),
    }))
}
