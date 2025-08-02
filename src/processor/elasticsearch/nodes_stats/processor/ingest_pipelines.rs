use super::super::super::{
    ElasticsearchMetadata, Metadata,
    nodes::NodeDocument,
    nodes_stats::{IngestPipelines, IngestProcessor, IngestProcessorStats, IngestProcessors},
};
use json_patch::merge;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Value, json};

/// Extract ingest.pipelines
pub fn extract(
    pipelines: Option<IngestPipelines>,
    metadata: &ElasticsearchMetadata,
    node_summary: Option<&NodeDocument>,
) -> Vec<Value> {
    let ingest_pipeline_metadata = metadata
        .for_data_stream("metrics-ingest.pipeline-esdiag")
        .as_meta_doc();

    let pipelines: Vec<Value> = match pipelines {
        Some(pipelines) => pipelines
            .into_iter()
            .collect::<Vec<_>>()
            .par_drain(..)
            .flat_map(|(name, mut pipeline)| {
                let processors = extract_ingest_processors(
                    &name,
                    pipeline.processors.take(),
                    metadata,
                    node_summary.cloned(),
                );

                let mut doc = json!({
                    "node": node_summary,
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
                let mut docs: Vec<Value> = vec![doc];
                docs.extend(processors);
                docs
            })
            .collect(),
        None => Vec::new(),
    };

    log::trace!("pipelines: {}", pipelines.len());
    pipelines
}

/// Extract ingest.processors
fn extract_ingest_processors(
    pipeline_name: &str,
    processors: Option<IngestProcessors>,
    metadata: &ElasticsearchMetadata,
    node_summary: Option<NodeDocument>,
) -> Vec<Value> {
    let ingest_processor_metadata = metadata
        .for_data_stream("metrics-ingest.processor-esdiag")
        .as_meta_doc();
    let processors: Vec<Value> = match processors {
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
                    metadata: ingest_processor_metadata.clone(),
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

    log::trace!("processors: {}", processors.len());
    processors
}

#[derive(Serialize)]
struct IngestDoc {
    #[serde(flatten)]
    metadata: Value,
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
