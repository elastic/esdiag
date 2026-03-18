// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::diagnostic::DataSource;
use super::{AgentMetadata, Metadata};
use crate::{exporter::Exporter, processor::ProcessorSummary};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DATA_STREAM: &str = "metrics-agent.input-esdiag";

/// Array of per-input metrics from `input_metrics.json`.
/// Each entry self-identifies via `input` (component type) and `id` (stream ID).
#[derive(Deserialize, Serialize)]
pub struct InputMetrics(Vec<Value>);

impl DataSource for InputMetrics {
    fn name() -> String {
        "input_metrics".to_string()
    }
}

impl InputMetrics {
    pub fn into_docs(self, metadata: &AgentMetadata) -> Vec<Value> {
        let meta = metadata.for_data_stream(DATA_STREAM).as_meta_doc();
        self.0
            .into_iter()
            .map(|entry| {
                json!(InputMetricDoc {
                    metadata: meta.clone(),
                    metrics: entry,
                })
            })
            .collect()
    }
}

pub async fn export_input_metrics(
    docs: Vec<Value>,
    exporter: &Exporter,
) -> ProcessorSummary {
    let mut summary = ProcessorSummary::new(DATA_STREAM.to_string());
    if docs.is_empty() {
        return summary;
    }
    match exporter.send(DATA_STREAM.to_string(), docs).await {
        Ok(batch) => summary.add_batch(batch),
        Err(err) => log::error!("Failed to send input metrics: {}", err),
    }
    summary
}

#[derive(Serialize)]
struct InputMetricDoc {
    #[serde(flatten)]
    metadata: Value,
    #[serde(flatten)]
    metrics: Value,
}
