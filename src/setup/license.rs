// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::client::Client;
use eyre::{Result, eyre};
use reqwest::Method;
use serde_json::Value;
use std::collections::HashMap;

/// Whether an Elasticsearch deployment can serve Enterprise-only Kibana Agent Builder assets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentBuilderLicense {
    /// A trial or enterprise license is active.
    Active,
    /// The license does not include Agent Builder.
    Inactive,
    /// Serverless manages feature entitlements; there is no license to change.
    Managed,
}

impl AgentBuilderLicense {
    pub fn allows_agent_builder(self) -> bool {
        !matches!(self, Self::Inactive)
    }
}

/// Read the Agent Builder license state without changing it.
pub async fn agent_builder_license_status(client: &Client) -> Result<AgentBuilderLicense> {
    require_elasticsearch(client)?;
    if client.is_serverless().await? {
        return Ok(AgentBuilderLicense::Managed);
    }
    Ok(if agent_builder_license_is_active(&current_license(client).await?) {
        AgentBuilderLicense::Active
    } else {
        AgentBuilderLicense::Inactive
    })
}

/// Start a trial license and verify that Agent Builder became available.
pub async fn activate_trial_license(client: &Client) -> Result<()> {
    require_elasticsearch(client)?;
    tracing::info!("Starting Elasticsearch trial license for Kibana Agent Builder assets");
    let response = client
        .request(
            Method::POST,
            &HashMap::new(),
            "_license/start_trial?acknowledge=true",
            None,
        )
        .await?;
    let status = response.status();
    if !status.is_success() {
        return Err(eyre!(
            "Failed to start Elasticsearch trial license ({status}): {}",
            response.text().await?
        ));
    }

    if agent_builder_license_is_active(&current_license(client).await?) {
        Ok(())
    } else {
        Err(eyre!(
            "Elasticsearch trial license did not become active; Kibana Agent Builder assets will not be loaded"
        ))
    }
}

fn require_elasticsearch(client: &Client) -> Result<()> {
    match client {
        Client::Elasticsearch(_) => Ok(()),
        _ => Err(eyre!(
            "an Elasticsearch client is required to read or change the license"
        )),
    }
}

pub(super) async fn current_license(client: &Client) -> Result<Value> {
    let response = client.request(Method::GET, &HashMap::new(), "_license", None).await?;
    let status = response.status();
    if !status.is_success() {
        return Err(eyre!(
            "Failed to read Elasticsearch license ({status}): {}",
            response.text().await?
        ));
    }
    response.json().await
}

pub(super) fn agent_builder_license_is_active(response: &Value) -> bool {
    let license = response.get("license").unwrap_or(response);
    license.get("status").and_then(Value::as_str) == Some("active")
        && matches!(
            license.get("type").and_then(Value::as_str),
            Some("trial" | "enterprise")
        )
}
