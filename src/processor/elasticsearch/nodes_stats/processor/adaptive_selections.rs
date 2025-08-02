use super::super::super::{Lookup, nodes::NodeDocument};
use super::{ElasticsearchMetadata, Metadata};
use json_patch::merge;
use rayon::prelude::*;
use serde_json::{Value, json};

/// Extract adaptive_selection
pub fn extract(
    adaptive_selection: Option<Value>,
    metadata: &ElasticsearchMetadata,
    node_summary: Option<&NodeDocument>,
    lookup_node: &Lookup<NodeDocument>,
) -> Vec<Value> {
    let adaptive_selection_metadata = metadata
        .for_data_stream("metrics-node.adaptive_selection-esdiag")
        .as_meta_doc();

    let adaptive_selections: Vec<_> = match adaptive_selection {
        Some(Value::Object(data)) => data
            .into_iter()
            .collect::<Vec<_>>()
            .par_drain(..)
            .map(|(peer_node_id, adaptive_selection)| {
                let mut doc = json!({
                    "adaptive_selection": adaptive_selection,
                    "node": node_summary,
                });

                let peer_node_patch = json!({
                    "adaptive_selection": {
                        "node": lookup_node.by_id(&peer_node_id),
                    },
                });

                merge(&mut doc, &peer_node_patch);
                merge(&mut doc, &adaptive_selection_metadata);
                doc
            })
            .collect(),
        None | _ => Vec::new(),
    };
    log::trace!("adaptive_selections: {}", adaptive_selections.len());

    adaptive_selections
}
