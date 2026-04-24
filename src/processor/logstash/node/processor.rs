// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, LogstashMetadata, Lookups, Metadata};
use super::{Node, Pipeline};
use crate::{exporter::Exporter, processor::ProcessorSummary};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;

impl DocumentExporter<Lookups, LogstashMetadata> for Node {
    async fn documents_export(
        mut self,
        exporter: &Exporter,
        lookups: &Lookups,
        metadata: &LogstashMetadata,
    ) -> ProcessorSummary {
        let mut docs: Vec<Value> = Vec::new();
        let data_stream = "settings-logstash.node-esdiag".to_string();
        let mut pipeline_docs = generate_pipeline_docs(metadata, self.take_pipelines());
        docs.append(&mut pipeline_docs);

        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let node_doc = json!(LogstashNodeDoc::new(self, metadata_doc, lookups.plugin_count));
        docs.push(node_doc);

        let mut summary = ProcessorSummary::new(data_stream.clone());
        match exporter.send(data_stream, docs).await {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => tracing::error!("Failed to send node settings: {}", err),
        }
        summary
    }
}

#[derive(Serialize)]
struct LogstashNodeDoc {
    #[serde(flatten)]
    metadata: Value,
    node: Value,
    plugins: Count,
    pipelines: Count,
}

#[derive(Serialize)]
struct Count {
    count: u32,
}

impl LogstashNodeDoc {
    fn new(node: Node, metadata: Value, plugin_count: u32) -> Self {
        let pipeline_count = node.get_pipeline_count();
        let mut node_with_metadata = json!(metadata.get("node"));
        json_patch::merge(&mut node_with_metadata, &json!(node));

        Self {
            metadata,
            node: node_with_metadata,
            plugins: Count { count: plugin_count },
            pipelines: Count { count: pipeline_count },
        }
    }
}

fn generate_pipeline_docs(metadata: &LogstashMetadata, pipelines: HashMap<String, Pipeline>) -> Vec<Value> {
    let metadata = metadata
        .for_data_stream("settings-logstash.pipeline-esdiag")
        .as_meta_doc();
    pipelines
        .into_iter()
        .map(|(name, pipeline)| json!(PipelineDoc::new(name, pipeline, metadata.clone())))
        .collect()
}

#[derive(Serialize)]
struct PipelineDoc {
    #[serde(flatten)]
    metadata: Value,
    pipeline: NamedPipeline,
}

#[derive(Serialize)]
struct NamedPipeline {
    name: String,
    #[serde(flatten)]
    pipeline: Pipeline,
}

impl PipelineDoc {
    fn new(name: String, pipeline: Pipeline, metadata: Value) -> Self {
        Self {
            metadata,
            pipeline: NamedPipeline { name, pipeline },
        }
    }
}
