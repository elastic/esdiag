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
use rayon::prelude::*;
use serde::Serialize;
use tokio::sync::mpsc::Sender;

/// Extract ingest.pipelines
pub async fn extract(
    sender_pipelines: &Sender<IngestPipelineDoc>,
    sender_processors: &Sender<IngestDoc>,
    mut pipelines: Option<IngestPipelines>,
    metadata: &ElasticsearchMetadata,
    node_metadata: Option<&NodeDocument>,
) -> Result<()> {
    let ingest_pipeline_metadata = metadata.for_data_stream("metrics-ingest.pipeline-esdiag");

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

            sender_pipelines
                .send(IngestPipelineDoc {
                    node: node_metadata.cloned(),
                    metadata: ingest_pipeline_metadata.clone(),
                    ingest: IngestPipelineContainer {
                        pipeline: NamedIngestPipeline { name, pipeline },
                    },
                })
                .await?;
        }
    }

    Ok(())
}

/// Extract ingest.processors
async fn extract_ingest_processors(
    sender: &Sender<IngestDoc>,
    processors: Option<IngestProcessors>,
    pipeline_name: &str,
    metadata: MetadataRawValue,
    node_summary: Option<NodeDocument>,
) -> Result<()> {
    let docs: Vec<IngestDoc> = match processors {
        Some(mut processors) => processors
            .par_drain(..)
            .enumerate()
            .filter_map(|(index, mut processor)| {
                processor.drain().next().map(|(name, processor)| IngestDoc {
                    metadata: metadata.clone(),
                    node: node_summary.clone(),
                    ingest: IngestProcessorDoc {
                        pipeline: IngestPipelineName {
                            name: pipeline_name.to_string(),
                        },
                        processor: IngestProcessorStatsDoc::from(processor)
                            .with_name(name)
                            .with_order(index),
                    },
                })
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
pub struct IngestDoc {
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

#[derive(Serialize)]
pub struct IngestPipelineDoc {
    #[serde(flatten)]
    metadata: MetadataRawValue,
    node: Option<NodeDocument>,
    ingest: IngestPipelineContainer,
}

#[derive(Serialize)]
struct IngestPipelineContainer {
    pipeline: NamedIngestPipeline,
}

#[derive(Serialize)]
struct NamedIngestPipeline {
    name: String,
    #[serde(flatten)]
    pipeline: super::super::super::nodes_stats::IngestPipeline,
}
