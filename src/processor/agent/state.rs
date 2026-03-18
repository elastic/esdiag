// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::diagnostic::DataSource;
use super::{AgentMetadata, DocumentExporter, Lookups, Metadata};
use crate::{exporter::Exporter, processor::ProcessorSummary};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DATA_STREAM: &str = "metrics-agent.state-esdiag";

#[derive(Deserialize, Serialize)]
pub struct State {
    #[serde(default)]
    pub components: Vec<StateComponent>,
    pub fleet_message: Option<String>,
    pub fleet_state: Option<u32>,
    pub log_level: Option<String>,
    pub message: Option<String>,
    pub state: Option<u32>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct StateComponent {
    pub id: String,
    pub state: Option<StateDetails>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct StateDetails {
    pub message: Option<String>,
    pub pid: Option<u32>,
    pub state: Option<u32>,
    pub units: Option<Value>,
    pub version_info: Option<Value>,
    pub component: Option<Value>,
    pub component_idx: Option<u32>,
    pub features_idx: Option<u32>,
}

impl State {
    pub fn component_ids(&self) -> Vec<String> {
        self.components.iter().map(|c| c.id.clone()).collect()
    }
}

impl DataSource for State {
    fn name() -> String {
        "state".to_string()
    }
}

impl DocumentExporter<Lookups, AgentMetadata> for State {
    async fn documents_export(
        self,
        exporter: &Exporter,
        _lookups: &Lookups,
        metadata: &AgentMetadata,
    ) -> ProcessorSummary {
        let meta = metadata.for_data_stream(DATA_STREAM).as_meta_doc();
        let mut docs: Vec<Value> = Vec::new();

        // Agent-level state document
        let agent_state = json!({
            "fleet_message": self.fleet_message,
            "fleet_state": self.fleet_state,
            "log_level": self.log_level,
            "message": self.message,
            "state": self.state,
            "component_count": self.components.len(),
        });
        docs.push(json!(StateDoc {
            metadata: meta.clone(),
            agent_state,
        }));

        // Per-component state documents
        for comp in self.components {
            let mut doc = json!(ComponentStateDoc {
                metadata: meta.clone(),
                component: comp.id,
                component_state: comp.state,
            });
            strip_empty_keys(&mut doc);
            docs.push(doc);
        }

        let mut summary = ProcessorSummary::new(DATA_STREAM.to_string());
        match exporter.send(DATA_STREAM.to_string(), docs).await {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => log::error!("Failed to send agent state: {}", err),
        }
        summary
    }
}

#[derive(Serialize)]
struct StateDoc {
    #[serde(flatten)]
    metadata: Value,
    #[serde(skip_serializing_if = "Value::is_null")]
    agent_state: Value,
}

#[derive(Serialize)]
struct ComponentStateDoc {
    #[serde(flatten)]
    metadata: Value,
    component: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    component_state: Option<StateDetails>,
}

/// Recursively remove empty-string keys from JSON objects.
/// Elasticsearch rejects documents with empty field names.
fn strip_empty_keys(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|k, _| !k.is_empty());
            for v in map.values_mut() {
                strip_empty_keys(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_empty_keys(v);
            }
        }
        _ => {}
    }
}
