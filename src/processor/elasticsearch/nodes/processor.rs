// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, ElasticsearchMetadata, Lookups, ProcessorSummary};
use super::super::metadata::MetadataRawValue;
use super::{Node, Nodes};
use crate::exporter::Exporter;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Map, Value};

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
            .map(|(node_id, node)| {
                let mut node_doc = serde_json::to_value(node_doc.clone().with_node(node))
                    .unwrap_or(Value::Null);
                if let Some(node_val) = node_doc.get_mut("node") {
                    set_nested_null(node_val, &["settings", "http", "type.default"]);
                    set_nested_null(node_val, &["settings", "transport", "type.default"]);

                    if let Some(summary) = lookup_node.by_id(&node_id)
                        && let Ok(summary_val) = serde_json::to_value(summary)
                    {
                        merge_values(node_val, &summary_val);
                    }
                }
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

fn merge_values(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target_obj), Value::Object(patch_obj)) => {
            for (key, value) in patch_obj {
                if let Some(existing) = target_obj.get_mut(key) {
                    merge_values(existing, value);
                } else {
                    target_obj.insert(key.clone(), value.clone());
                }
            }
        }
        (target, patch) => {
            *target = patch.clone();
        }
    }
}

fn set_nested_null(root: &mut Value, path: &[&str]) {
    if path.is_empty() {
        return;
    }

    if !root.is_object() {
        *root = Value::Object(Map::new());
    }

    let object = root.as_object_mut().expect("initialized object");
    if path.len() == 1 {
        object.insert(path[0].to_string(), Value::Null);
        return;
    }

    let child = object
        .entry(path[0].to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    set_nested_null(child, &path[1..]);
}
