// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::diagnostic::DataSource;
use serde::{Deserialize, Serialize};

/// The root structure of `agent-info.yaml`.
/// This is the identity/bootstrap file for Agent diagnostics.
#[derive(Deserialize, Serialize, Clone)]
pub struct AgentInfo {
    pub metadata: Option<AgentInfoMetadata>,
    pub log_level: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct AgentInfoMetadata {
    pub elastic: Option<ElasticMetadata>,
    pub host: Option<HostMetadata>,
    pub os: Option<OsMetadata>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ElasticMetadata {
    pub agent: Option<AgentIdentity>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct AgentIdentity {
    pub id: Option<String>,
    pub version: Option<String>,
    pub snapshot: Option<bool>,
    pub unprivileged: Option<bool>,
    pub buildoriginal: Option<String>,
    pub complete: Option<bool>,
    pub loglevel: Option<String>,
    pub upgradeable: Option<bool>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct HostMetadata {
    pub arch: Option<String>,
    pub hostname: Option<String>,
    pub id: Option<String>,
    pub ip: Option<Vec<String>>,
    pub mac: Option<Vec<String>>,
    pub name: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct OsMetadata {
    pub family: Option<String>,
    pub fullname: Option<String>,
    pub kernel: Option<String>,
    pub name: Option<String>,
    pub platform: Option<String>,
    pub version: Option<String>,
}

impl AgentInfo {
    pub fn agent_id(&self) -> String {
        self.metadata
            .as_ref()
            .and_then(|m| m.elastic.as_ref())
            .and_then(|e| e.agent.as_ref())
            .and_then(|a| a.id.clone())
            .unwrap_or_default()
    }

    pub fn hostname(&self) -> String {
        self.metadata
            .as_ref()
            .and_then(|m| m.host.as_ref())
            .and_then(|h| h.hostname.clone())
            .unwrap_or_else(|| "agent".to_string())
    }

    pub fn version(&self) -> Option<String> {
        self.metadata
            .as_ref()
            .and_then(|m| m.elastic.as_ref())
            .and_then(|e| e.agent.as_ref())
            .and_then(|a| a.version.clone())
    }
}

impl DataSource for AgentInfo {
    fn name() -> String {
        "agent-info".to_string()
    }
}
