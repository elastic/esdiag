// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, LogstashMetadata, Lookups, Metadata};
use super::{Plugin, Plugins};
use crate::processor::BatchResponse;
use crate::{exporter::Exporter, processor::ProcessorSummary};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;

impl DocumentExporter<Lookups, LogstashMetadata> for Plugins {
    async fn documents_export(
        self,
        exporter: &Exporter,
        _: &Lookups,
        metadata: &LogstashMetadata,
        batch_tx: mpsc::Sender<BatchResponse>,
    ) -> ProcessorSummary {
        let data_stream = "settings-logstash.plugin-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let docs: Vec<Value> = self
            .plugins
            .into_iter()
            .map(|plugin| json!(PluginDoc::new(plugin, metadata_doc.clone())))
            .collect();
        let mut summary = ProcessorSummary::new(data_stream.clone());
        match exporter.send(data_stream, docs).await {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => log::error!("Failed to send plugins: {}", err),
        }
        summary
    }
}

#[derive(Serialize)]
struct PluginDoc {
    #[serde(flatten)]
    metadata: Value,
    plugin: Plugin,
}

impl PluginDoc {
    fn new(plugin: Plugin, metadata: Value) -> Self {
        Self { metadata, plugin }
    }
}
