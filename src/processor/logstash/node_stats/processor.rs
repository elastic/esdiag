// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, LogstashMetadata, Lookups, Metadata};
use super::{NodeStats, PipelinePlugins, PipelineStats};
use crate::processor::BatchResponse;
use crate::{exporter::Exporter, processor::ProcessorSummary};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use tokio::sync::mpsc;

impl DocumentExporter<Lookups, LogstashMetadata> for NodeStats {
    async fn documents_export(
        mut self,
        exporter: &Exporter,
        _: &Lookups,
        metadata: &LogstashMetadata,
        batch_tx: mpsc::Sender<BatchResponse>,
    ) -> ProcessorSummary {
        let mut docs: Vec<Value> = Vec::new();
        self.take_pipelines().map(|pipelines| {
            let mut pipeline_docs = generate_pipeline_docs(metadata, pipelines);
            docs.append(&mut pipeline_docs);
        });

        let data_stream = "metrics-logstash.node-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let node_doc = json!(NodeStatsDoc::new(self, metadata_doc));
        docs.push(node_doc);

        let mut summary = ProcessorSummary::new(data_stream.clone());
        match exporter.send(data_stream, docs).await {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => log::error!("Failed to send node stats: {}", err),
        }
        summary
    }
}

#[derive(Serialize)]
struct NodeStatsDoc {
    #[serde(flatten)]
    metadata: Value,
    node: Value,
}

impl NodeStatsDoc {
    fn new(node: NodeStats, metadata: Value) -> Self {
        let mut node_with_metadata = json!(metadata.get("node").take());
        json_patch::merge(&mut node_with_metadata, &json!(node));

        Self {
            metadata,
            node: node_with_metadata,
        }
    }
}

fn generate_pipeline_docs(
    metadata: &LogstashMetadata,
    pipelines: HashMap<String, PipelineStats>,
) -> Vec<Value> {
    let pipeline_metadata_doc = metadata
        .for_data_stream("metrics-logstash.pipeline-esdiag")
        .as_meta_doc();

    let mut plugin_docs: Vec<Value> = Vec::new();
    let mut pipeline_docs: Vec<Value> = pipelines
        .into_iter()
        .map(|(name, mut stats)| {
            stats.take_plugins().map(|plugins| {
                let mut docs = generate_plugin_docs(metadata, plugins);
                plugin_docs.append(&mut docs);
            });
            json!(PipelineDoc::new(name, stats, pipeline_metadata_doc.clone()))
        })
        .collect();

    pipeline_docs.append(&mut plugin_docs);
    pipeline_docs
}

#[derive(Serialize)]
struct PipelineDoc {
    #[serde(flatten)]
    metadata: Value,
    pipeline: NamedPipelineStats,
}

#[derive(Serialize)]
struct NamedPipelineStats {
    name: String,
    #[serde(flatten)]
    stats: PipelineStats,
}

impl PipelineDoc {
    fn new(name: String, stats: PipelineStats, metadata: Value) -> Self {
        Self {
            metadata,
            pipeline: NamedPipelineStats { name, stats },
        }
    }
}

#[derive(Serialize)]
struct PluginDoc {
    #[serde(flatten)]
    metadata: Value,
    plugin: TypedPluginStats,
}

#[derive(Serialize)]
struct TypedPluginStats {
    r#type: String,
    #[serde(flatten)]
    stats: Value,
}

impl PluginDoc {
    fn new(plugin_type: String, stats: Value, metadata: Value) -> Self {
        Self {
            metadata,
            plugin: TypedPluginStats {
                r#type: plugin_type,
                stats,
            },
        }
    }
}

fn generate_plugin_docs(metadata: &LogstashMetadata, plugins: PipelinePlugins) -> Vec<Value> {
    let plugin_metadata_doc = metadata
        .for_data_stream("metrics-logstash.plugin-esdiag")
        .as_meta_doc();

    let mut docs: Vec<Value> = Vec::new();

    let mut input_docs = plugins
        .inputs
        .into_iter()
        .map(|stats| {
            json!(PluginDoc::new(
                "input".to_string(),
                json!(stats),
                plugin_metadata_doc.clone()
            ))
        })
        .collect::<Vec<Value>>();

    let mut codec_docs = plugins
        .codecs
        .into_iter()
        .map(|stats| {
            json!(PluginDoc::new(
                "codec".to_string(),
                json!(stats),
                plugin_metadata_doc.clone()
            ))
        })
        .collect::<Vec<Value>>();

    let mut filter_docs = plugins
        .filters
        .into_iter()
        .map(|stats| {
            json!(PluginDoc::new(
                "filter".to_string(),
                json!(stats),
                plugin_metadata_doc.clone()
            ))
        })
        .collect::<Vec<Value>>();

    let mut output_docs = plugins
        .outputs
        .into_iter()
        .map(|stats| {
            json!(PluginDoc::new(
                "output".to_string(),
                json!(stats),
                plugin_metadata_doc.clone()
            ))
        })
        .collect::<Vec<Value>>();

    docs.append(&mut input_docs);
    docs.append(&mut codec_docs);
    docs.append(&mut filter_docs);
    docs.append(&mut output_docs);
    docs
}
