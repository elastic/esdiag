// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::diagnostic::DataSource;
use super::{AgentMetadata, Metadata};
use crate::{exporter::Exporter, processor::ProcessorSummary};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Parsed `beat-rendered-config.yml` split by top-level key.
#[derive(Deserialize, Serialize)]
pub struct RenderedConfig {
    #[serde(default)]
    pub inputs: Vec<Value>,
    #[serde(default)]
    pub outputs: Option<Value>,
    #[serde(default)]
    pub features: Option<Value>,
    #[serde(default)]
    pub apm: Option<Value>,
}

impl DataSource for RenderedConfig {
    fn name() -> String {
        "beat-rendered-config".to_string()
    }
}

impl RenderedConfig {
    pub fn into_docs(self, component_id: &str, metadata: &AgentMetadata) -> RenderedConfigDocs {
        let mut inputs_docs = Vec::new();
        let inputs_meta = metadata
            .for_data_stream("settings-agent.inputs-esdiag")
            .as_meta_doc();
        for input in self.inputs {
            inputs_docs.push(json!(ComponentDoc {
                metadata: inputs_meta.clone(),
                component: component_id.to_string(),
                data: input,
            }));
        }

        let mut outputs_docs = Vec::new();
        if let Some(outputs) = self.outputs {
            let outputs_meta = metadata
                .for_data_stream("settings-agent.outputs-esdiag")
                .as_meta_doc();
            outputs_docs.push(json!(ComponentDoc {
                metadata: outputs_meta,
                component: component_id.to_string(),
                data: outputs,
            }));
        }

        let mut features_docs = Vec::new();
        if let Some(features) = self.features {
            let features_meta = metadata
                .for_data_stream("settings-agent.features-esdiag")
                .as_meta_doc();
            features_docs.push(json!(ComponentDoc {
                metadata: features_meta,
                component: component_id.to_string(),
                data: features,
            }));
        }

        let mut apm_docs = Vec::new();
        if let Some(apm) = self.apm {
            let apm_meta = metadata
                .for_data_stream("settings-agent.apm-esdiag")
                .as_meta_doc();
            apm_docs.push(json!(ComponentDoc {
                metadata: apm_meta,
                component: component_id.to_string(),
                data: apm,
            }));
        }

        RenderedConfigDocs {
            inputs: inputs_docs,
            outputs: outputs_docs,
            features: features_docs,
            apm: apm_docs,
        }
    }
}

pub struct RenderedConfigDocs {
    pub inputs: Vec<Value>,
    pub outputs: Vec<Value>,
    pub features: Vec<Value>,
    pub apm: Vec<Value>,
}

pub async fn export_rendered_configs(
    all_docs: RenderedConfigDocs,
    exporter: &Exporter,
) -> Vec<ProcessorSummary> {
    let mut summaries = Vec::new();

    for (stream, docs) in [
        ("settings-agent.inputs-esdiag", all_docs.inputs),
        ("settings-agent.outputs-esdiag", all_docs.outputs),
        ("settings-agent.features-esdiag", all_docs.features),
        ("settings-agent.apm-esdiag", all_docs.apm),
    ] {
        let mut summary = ProcessorSummary::new(stream.to_string());
        if !docs.is_empty() {
            match exporter.send(stream.to_string(), docs).await {
                Ok(batch) => summary.add_batch(batch),
                Err(err) => log::error!("Failed to send {}: {}", stream, err),
            }
        }
        summaries.push(summary);
    }

    summaries
}

#[derive(Serialize)]
struct ComponentDoc {
    #[serde(flatten)]
    metadata: Value,
    component: String,
    #[serde(flatten)]
    data: Value,
}
