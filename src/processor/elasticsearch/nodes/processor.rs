// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, ElasticsearchMetadata, Lookups, Metadata, ProcessorSummary};
use super::{Node, Nodes};
use crate::exporter::Exporter;
use crate::processor::BatchResponse;
use json_patch::merge;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;

impl DocumentExporter<Lookups, ElasticsearchMetadata> for Nodes {
    async fn documents_export(
        self,
        exporter: &Exporter,
        lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
        batch_tx: mpsc::Sender<BatchResponse>,
    ) -> ProcessorSummary {
        let mut nodes = self.nodes;
        log::debug!("nodes: {}", nodes.len());
        let data_stream = "settings-node-esdiag".to_string();
        let lookup_node = &lookups.node;
        let metadata = metadata.for_data_stream(&data_stream).as_meta_doc();

        let node_doc = NodeDoc {
            metadata,
            node: None,
        };

        let node_docs: Vec<Value> = nodes
            .par_drain()
            .map(|(node_id, node)| {
                let patch = json!({
                    "node" : {
                        "settings": {
                            "http": {
                                "type.default": null,
                            },
                            "transport": {
                                "type.default": null,
                            },
                        }
                    }
                });

                let node_summary = json!({"node": lookup_node.by_id(&node_id)});
                let mut node_doc = json!(node_doc.clone().with_node(node));

                merge(&mut node_doc, &patch);
                merge(&mut node_doc, &node_summary);
                node_doc
            })
            .collect();

        log::debug!("node docs: {}", node_docs.len());
        let mut summary = ProcessorSummary::new(data_stream.clone());
        match exporter.send(data_stream, node_docs).await {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => log::error!("Failed to send nodes: {}", err),
        }
        summary
    }
}

#[derive(Clone, Serialize)]
struct NodeDoc {
    #[serde(flatten)]
    metadata: Value,
    node: Option<Node>,
}

impl NodeDoc {
    fn with_node(self, node: Node) -> Self {
        Self {
            node: Some(node),
            ..self
        }
    }
}
