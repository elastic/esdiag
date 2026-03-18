// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::diagnostic::DataSource;
use super::{AgentMetadata, DocumentExporter, Lookups, Metadata};
use crate::{exporter::Exporter, processor::ProcessorSummary};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DATA_STREAM: &str = "settings-agent.local_config-esdiag";

/// Opaque passthrough for `local-config.yaml`.
#[derive(Deserialize, Serialize)]
pub struct LocalConfig(Value);

impl DataSource for LocalConfig {
    fn name() -> String {
        "local-config".to_string()
    }
}

impl DocumentExporter<Lookups, AgentMetadata> for LocalConfig {
    async fn documents_export(
        self,
        exporter: &Exporter,
        _lookups: &Lookups,
        metadata: &AgentMetadata,
    ) -> ProcessorSummary {
        let meta = metadata.for_data_stream(DATA_STREAM).as_meta_doc();
        let doc = json!(ConfigDoc {
            metadata: meta,
            config: self.0,
        });

        let mut summary = ProcessorSummary::new(DATA_STREAM.to_string());
        match exporter.send(DATA_STREAM.to_string(), vec![doc]).await {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => log::error!("Failed to send local config: {}", err),
        }
        summary
    }
}

#[derive(Serialize)]
struct ConfigDoc {
    #[serde(flatten)]
    metadata: Value,
    config: Value,
}
