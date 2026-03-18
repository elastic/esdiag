// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::diagnostic::{DataStreamName, DiagnosticManifest, DiagnosticMetadata};
use super::{Metadata, agent_info::AgentInfo};
use eyre::Result;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Serialize)]
pub struct AgentMetadata {
    pub agent_info: AgentInfo,
    pub diagnostic: DiagnosticMetadata,
    pub timestamp: u64,
    pub as_doc: MetadataDoc,
}

#[derive(Clone, Serialize)]
pub struct MetadataDoc {
    #[serde(rename = "@timestamp")]
    pub timestamp: u64,
    pub agent: AgentMetaFields,
    pub host: HostMetaFields,
    pub os: OsMetaFields,
    pub diagnostic: DiagnosticMetadata,
    pub data_stream: DataStreamName,
}

#[derive(Clone, Serialize)]
pub struct AgentMetaFields {
    pub id: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unprivileged: Option<bool>,
}

#[derive(Clone, Serialize)]
pub struct HostMetaFields {
    pub hostname: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<Vec<String>>,
}

#[derive(Clone, Serialize)]
pub struct OsMetaFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel: Option<String>,
}

impl Metadata for MetadataDoc {
    fn as_meta_doc(&self) -> Value {
        serde_json::to_value(self).expect("Failed to serialize metadata")
    }
}

impl AgentMetadata {
    pub fn try_new(manifest: DiagnosticManifest, agent_info: AgentInfo) -> Result<Self> {
        let hostname = agent_info.hostname();
        let diagnostic = DiagnosticMetadata::try_from(manifest.with_name(hostname.clone()))?;
        let timestamp = diagnostic.collection_date;

        let agent_identity = agent_info
            .metadata
            .as_ref()
            .and_then(|m| m.elastic.as_ref())
            .and_then(|e| e.agent.as_ref());
        let host_meta = agent_info.metadata.as_ref().and_then(|m| m.host.as_ref());
        let os_meta = agent_info.metadata.as_ref().and_then(|m| m.os.as_ref());

        let as_doc = MetadataDoc {
            timestamp,
            agent: AgentMetaFields {
                id: agent_identity
                    .and_then(|a| a.id.clone())
                    .unwrap_or_default(),
                version: agent_identity
                    .and_then(|a| a.version.clone())
                    .unwrap_or_default(),
                snapshot: agent_identity.and_then(|a| a.snapshot),
                unprivileged: agent_identity.and_then(|a| a.unprivileged),
            },
            host: HostMetaFields {
                hostname: hostname.clone(),
                name: host_meta
                    .and_then(|h| h.name.clone())
                    .unwrap_or_else(|| hostname.clone()),
                arch: host_meta.and_then(|h| h.arch.clone()),
                ip: host_meta.and_then(|h| h.ip.clone()),
            },
            os: OsMetaFields {
                family: os_meta.and_then(|o| o.family.clone()),
                name: os_meta.and_then(|o| o.name.clone()),
                platform: os_meta.and_then(|o| o.platform.clone()),
                version: os_meta.and_then(|o| o.version.clone()),
                kernel: os_meta.and_then(|o| o.kernel.clone()),
            },
            diagnostic: diagnostic.clone(),
            data_stream: DataStreamName::from("metrics-default-esdiag"),
        };

        Ok(Self {
            as_doc,
            agent_info,
            diagnostic,
            timestamp,
        })
    }

    pub fn for_data_stream(&self, data_stream: &str) -> MetadataDoc {
        MetadataDoc {
            data_stream: DataStreamName::from(data_stream),
            ..self.as_doc.clone()
        }
    }
}
