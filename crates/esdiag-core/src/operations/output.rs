// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{
    cli_output::{CliOutcome, CommandOutcome},
    data::{ApplicationConfig, ElasticOutputContext, OutputDeployment},
};
use eyre::Result;

/// Persists the Elastic CLI context used as the default output.
pub fn output_set(context: String) -> Result<CommandOutcome> {
    let mut config = ApplicationConfig::load()?;
    let reference = ElasticOutputContext::new(context)?;
    OutputDeployment::from_elastic_context(&reference, false)?;
    config.output.elastic_context = Some(reference.clone());
    config.save()?;
    Ok(CommandOutcome::Completed(CliOutcome::OutputContext {
        operation: "set".to_string(),
        context: Some(reference.context),
    }))
}

pub fn output_show() -> Result<CommandOutcome> {
    let config = ApplicationConfig::load()?;
    Ok(CommandOutcome::Completed(CliOutcome::OutputContext {
        operation: "show".to_string(),
        context: config.output.elastic_context.map(|value| value.context),
    }))
}

pub fn output_clear() -> Result<CommandOutcome> {
    let mut config = ApplicationConfig::load()?;
    config.output.elastic_context = None;
    config.save()?;
    Ok(CommandOutcome::Completed(CliOutcome::OutputContext {
        operation: "clear".to_string(),
        context: None,
    }))
}
