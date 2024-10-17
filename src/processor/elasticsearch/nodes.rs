use super::{DataProcessor, ElasticsearchMetadata, Lookups};
use crate::{
    data::elasticsearch::{Node, Nodes},
    processor::Metadata,
};
use json_patch::merge;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;

impl DataProcessor<ElasticsearchMetadata> for Nodes {
    fn generate_docs(
        self,
        lookups: Arc<Lookups>,
        metadata: Arc<ElasticsearchMetadata>,
    ) -> (String, Vec<Value>) {
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

        log::debug!("node settings docs: {}", node_docs.len());
        (data_stream, node_docs)
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
