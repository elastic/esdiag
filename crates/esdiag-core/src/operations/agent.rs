// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{
    agent::builder::{AgentBuilderClient, AgentBuilderLocation, AgentFailure, AgentProgress, AgentRequest},
    cli_output::{AgentUsageResult, CliOutcome, CommandOutcome},
    client::KibanaClient,
    data::OutputDeployment,
};
use eyre::{Result, eyre};
use url::Url;

pub const DEFAULT_AGENT_BUILDER_AGENT: &str = "elastic-ai-agent";

#[derive(Debug, Clone)]
pub struct AgentAskInput {
    /// Passed unchanged to Agent Builder.
    pub prompt: String,
    pub agent_id: String,
    /// Continue this conversation; `None` starts a new one.
    pub conversation: Option<String>,
}

/// Asks the Kibana Agent Builder of the configured output deployment.
///
/// `progress` receives the readable agent name and each streamed event.
pub async fn agent_ask<F>(input: AgentAskInput, progress: F) -> Result<CommandOutcome>
where
    F: FnMut(&str, AgentProgress),
{
    let deployment = OutputDeployment::resolve(None, true)?;
    agent_ask_with_deployment(input, deployment, progress).await
}

pub(crate) async fn agent_ask_with_deployment<F>(
    input: AgentAskInput,
    deployment: OutputDeployment,
    mut progress: F,
) -> Result<CommandOutcome>
where
    F: FnMut(&str, AgentProgress),
{
    let AgentAskInput {
        prompt,
        agent_id: agent,
        conversation,
    } = input;
    let viewer = deployment
        .kibana
        .ok_or_else(|| eyre!("The configured output deployment has no Kibana viewer"))?;
    let viewer_auth = deployment
        .kibana_auth
        .ok_or_else(|| eyre!("The configured output deployment has no Kibana viewer authentication"))?;
    let viewer_url = viewer.get_url()?;
    let location = AgentBuilderLocation::new(viewer_url.clone(), agent_builder_space(&viewer_url));
    let client_url = if std::env::var("ESDIAG_CONTAINER_LOCAL_STACK").ok().as_deref() == Some("full") {
        std::env::var("ESDIAG_KIBANA_INTERNAL_URL")
            .ok()
            .map(|url| Url::parse(&url))
            .transpose()?
            .unwrap_or_else(|| viewer_url.clone())
    } else {
        viewer_url.clone()
    };
    let client = KibanaClient::try_new(client_url, viewer_auth)?;
    let agent_client = AgentBuilderClient::new(&client, location);
    let agent_name = agent_client
        .agent_name(&agent)
        .await
        .unwrap_or_else(|_| readable_agent_name(&agent));
    let request = AgentRequest {
        agent_id: agent,
        prompt,
        conversation_id: conversation,
    };
    let completion = agent_client
        .ask(request, |event| progress(&agent_name, event))
        .await
        .map_err(agent_failure_report)?;

    Ok(CommandOutcome::Completed(CliOutcome::AgentResponse {
        conversation_id: completion.conversation_id,
        message: completion.message,
        kibana_url: completion.kibana_url,
        usage: completion.usage.map(|usage| AgentUsageResult {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
        }),
    }))
}

fn agent_failure_report(error: AgentFailure) -> eyre::Report {
    eyre::Report::new(error)
}

/// Prefixes a follow-up prompt with the diagnostic it is about.
pub fn process_ask_prompt(diagnostic_id: &str, prompt: &str) -> String {
    format!("diagnostic.id: {diagnostic_id}\n{prompt}")
}

fn readable_agent_name(agent_id: &str) -> String {
    let words: Vec<_> = agent_id
        .split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| match word.to_ascii_lowercase().as_str() {
            "ai" => "AI".to_string(),
            "api" => "API".to_string(),
            _ => {
                let mut characters = word.chars();
                let Some(first) = characters.next() else {
                    return String::new();
                };
                format!("{}{}", first.to_uppercase(), characters.as_str())
            }
        })
        .collect();
    if words.is_empty() {
        agent_id.to_string()
    } else {
        words.join(" ")
    }
}

fn agent_builder_space(viewer: &Url) -> Option<String> {
    match std::env::var("ESDIAG_KIBANA_SPACE") {
        Ok(space) => {
            let space = space.trim();
            Some(if space.is_empty() { "_default" } else { space }.to_string())
        }
        Err(std::env::VarError::NotPresent) => {
            let viewer_has_space = viewer.path_segments().is_some_and(|mut segments| {
                while let Some(segment) = segments.next() {
                    if segment == "s" {
                        return segments.next().is_some();
                    }
                }
                false
            });
            (!viewer_has_space).then(|| crate::env::ESDIAG_KIBANA_DEFAULT_SPACE.to_string())
        }
        Err(_) => Some(crate::env::ESDIAG_KIBANA_DEFAULT_SPACE.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        TestEnv,
        cli_output::{CliFailureCategory, structured_failure},
    };

    #[test]
    fn process_ask_prefixes_the_completed_diagnostic_id() {
        assert_eq!(
            process_ask_prompt("prod-es@2026-08-18~a1b2", "What is the highest-risk finding?"),
            "diagnostic.id: prod-es@2026-08-18~a1b2\nWhat is the highest-risk finding?"
        );
    }

    #[test]
    fn unreadable_agent_names_fall_back_to_a_readable_agent_id() {
        assert_eq!(readable_agent_name("diagnostic-agent"), "Diagnostic Agent");
        assert_eq!(readable_agent_name("elastic-ai-agent"), "Elastic AI Agent");
        assert_eq!(readable_agent_name("custom_api_agent"), "Custom API Agent");
    }

    #[test]
    fn agent_builder_space_requires_a_space_segment_pair() {
        let mut env = TestEnv::new();
        env.remove("ESDIAG_KIBANA_SPACE");

        assert_eq!(
            agent_builder_space(&Url::parse("https://kb.example/app/s").expect("url")),
            Some("esdiag".to_string())
        );
        assert_eq!(
            agent_builder_space(&Url::parse("https://kb.example/app/s/support").expect("url")),
            None
        );
    }

    #[test]
    fn agent_builder_space_explicit_default_overrides_url() {
        let mut env = TestEnv::new();
        let viewer = Url::parse("https://kb.example/s/support").unwrap();
        for value in ["_default", "", "  _default  "] {
            env.set("ESDIAG_KIBANA_SPACE", value);
            let location = AgentBuilderLocation::new(viewer.clone(), agent_builder_space(&viewer));
            assert_eq!(location.space(), None);
        }
    }

    #[test]
    fn agent_builder_conflicts_are_conflict_failures() {
        let error = agent_failure_report(AgentFailure::Http { status: 409 });

        let failure = structured_failure(&error);
        let value = serde_json::to_value(failure).expect("serialize failure");

        assert_eq!(value["category"], CliFailureCategory::Conflict.as_str());
        assert_eq!(value["status"], 409);
    }
}
