// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::{
    ElasticsearchMetadata,
    metadata::MetadataRawValue,
    nodes::NodeDocument,
    nodes_stats::{IngestPipelines, IngestProcessor, IngestProcessorStats, IngestProcessors},
};
use eyre::Result;
use json_patch::merge;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::mpsc::Sender;

/// Extract ingest.pipelines
pub async fn extract(
    sender_pipelines: &Sender<Value>,
    sender_processors: &Sender<Value>,
    mut pipelines: Option<IngestPipelines>,
    metadata: &ElasticsearchMetadata,
    node_metadata: Option<&NodeDocument>,
) -> Result<()> {
    let ingest_pipeline_metadata = metadata
        .for_data_stream("metrics-ingest.pipeline-esdiag")
        .as_meta_doc();

    let ingest_processor_metadata = metadata.for_data_stream("metrics-ingest.processor-esdiag");

    if let Some(pipelines) = pipelines.take() {
        for (name, mut pipeline) in pipelines {
            if let Err(e) = extract_ingest_processors(
                sender_processors,
                pipeline.processors.take(),
                &name,
                ingest_processor_metadata.clone(),
                node_metadata.cloned(),
            )
            .await
            {
                log::error!("Error extracting ingest pipelines stats: {}", e);
            };

            let mut doc = json!({
                "node": node_metadata,
                "ingest": {
                    "pipeline": pipeline,
                },
            });

            let pipeline = json!({
                "ingest": {
                    "pipeline": {
                        "processors": null,
                        "name": name,
                    }
                }
            });

            merge(&mut doc, &pipeline);
            merge(&mut doc, &ingest_pipeline_metadata);
            sender_pipelines.send(doc).await?;
        }
    }

    Ok(())
}

/// Extract ingest.processors
async fn extract_ingest_processors(
    sender: &Sender<Value>,
    processors: Option<IngestProcessors>,
    pipeline_name: &str,
    metadata: MetadataRawValue,
    node_summary: Option<NodeDocument>,
) -> Result<()> {
    let docs: Vec<Value> = match processors {
        Some(mut processors) => processors
            .par_drain(..)
            .enumerate()
            .filter_map(|(index, mut processor)| {
                let processor = processor
                    .drain()
                    .next()
                    .map(|(name, processor)| {
                        IngestProcessorStatsDoc::from(processor)
                            .with_name(name)
                            .with_order(index)
                    })
                    .unwrap();
                serde_json::to_value(IngestDoc {
                    metadata: metadata.clone(),
                    node: node_summary.clone(),
                    ingest: IngestProcessorDoc {
                        pipeline: IngestPipelineName {
                            name: pipeline_name.to_string(),
                        },
                        processor,
                    },
                })
                .ok()
            })
            .collect(),
        None => Vec::new(),
    };

    for doc in docs {
        sender.send(doc).await?;
    }
    Ok(())
}

#[derive(Serialize)]
struct IngestDoc {
    #[serde(flatten)]
    metadata: MetadataRawValue,
    node: Option<NodeDocument>,
    ingest: IngestProcessorDoc,
}

#[derive(Serialize)]
struct IngestProcessorDoc {
    pipeline: IngestPipelineName,
    processor: IngestProcessorStatsDoc,
}

#[derive(Serialize)]
struct IngestProcessorStatsDoc {
    r#type: String,
    #[serde(flatten)]
    stats: IngestProcessorStats,
    order: Option<usize>,
    name: Option<String>,
}

impl IngestProcessorStatsDoc {
    fn with_name(self, name: String) -> Self {
        Self {
            name: Some(name),
            ..self
        }
    }

    fn with_order(self, order: usize) -> Self {
        Self {
            order: Some(order),
            ..self
        }
    }
}

impl From<IngestProcessor> for IngestProcessorStatsDoc {
    fn from(processor: IngestProcessor) -> Self {
        IngestProcessorStatsDoc {
            r#type: processor.r#type,
            stats: processor.stats,
            order: None,
            name: None,
        }
    }
}

#[derive(Serialize)]
struct IngestPipelineName {
    name: String,
}
