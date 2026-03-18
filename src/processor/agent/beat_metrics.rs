// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::diagnostic::DataSource;
use super::{AgentMetadata, Metadata};
use crate::{exporter::Exporter, processor::ProcessorSummary};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DATA_STREAM: &str = "metrics-agent.beat-esdiag";

/// RawValue passthrough for `beat_metrics.json`.
/// Deep JSON with beat/libbeat/system stats — schema varies by Beat type.
#[derive(Deserialize, Serialize)]
pub struct BeatMetrics {
    #[serde(flatten)]
    inner: Value,
}

impl DataSource for BeatMetrics {
    fn name() -> String {
        "beat_metrics".to_string()
    }
}

impl BeatMetrics {
    pub fn into_doc(self, component_id: &str, metadata: &AgentMetadata) -> Value {
        let meta = metadata.for_data_stream(DATA_STREAM).as_meta_doc();
        json!(BeatMetricsDoc {
            metadata: meta,
            component: component_id.to_string(),
            metrics: self.inner,
        })
    }
}

pub async fn export_beat_metrics(
    docs: Vec<Value>,
    exporter: &Exporter,
) -> ProcessorSummary {
    let mut summary = ProcessorSummary::new(DATA_STREAM.to_string());
    if docs.is_empty() {
        return summary;
    }
    match exporter.send(DATA_STREAM.to_string(), docs).await {
        Ok(batch) => summary.add_batch(batch),
        Err(err) => log::error!("Failed to send beat metrics: {}", err),
    }
    summary
}

#[derive(Serialize)]
struct BeatMetricsDoc {
    #[serde(flatten)]
    metadata: Value,
    component: String,
    #[serde(flatten)]
    metrics: Value,
}
