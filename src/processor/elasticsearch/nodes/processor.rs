// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, ElasticsearchMetadata, Lookups, ProcessorSummary};
use super::super::metadata::MetadataRawValue;
use super::{Node, Nodes};
use crate::exporter::Exporter;
use json_patch::merge;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;

impl DocumentExporter<Lookups, ElasticsearchMetadata> for Nodes {
    async fn documents_export(
        self,
        exporter: &Exporter,
        lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        let mut nodes = self.nodes;
        log::debug!("nodes: {}", nodes.len());
        let data_stream = "settings-node-esdiag".to_string();
        let lookup_node = &lookups.node;
        let metadata = metadata.for_data_stream(&data_stream);

        let node_doc = NodeDoc {
            metadata,
            node: None,
        };

        let node_docs: Vec<Value> = nodes
            .par_drain()
            .filter_map(|(node_id, node)| {
                let mut node_doc = match serde_json::to_value(node_doc.clone().with_node(node)) {
                    Ok(doc) => doc,
                    Err(err) => {
                        log::error!("Failed to serialize node document for {}: {}", node_id, err);
                        return None;
                    }
                };
                if let Some(node_val) = node_doc.get_mut("node") {
                    remove_nested_key(node_val, &["settings", "http", "type.default"]);
                    remove_nested_key(node_val, &["settings", "transport", "type.default"]);

                    if let Some(summary) = lookup_node.by_id(&node_id)
                        && let Ok(summary_val) = serde_json::to_value(summary)
                    {
                        merge(node_val, &summary_val);
                    }
                }
                Some(node_doc)
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
    metadata: MetadataRawValue,
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

fn remove_nested_key(root: &mut Value, path: &[&str]) {
    if path.is_empty() {
        return;
    }

    let Value::Object(object) = root else {
        return;
    };

    if path.len() == 1 {
        object.remove(path[0]);
        return;
    }

    if let Some(child) = object.get_mut(path[0]) {
        remove_nested_key(child, &path[1..]);
        let prune_child = child.as_object().is_some_and(|map| map.is_empty());
        if prune_child {
            object.remove(path[0]);
        }
    }
}
