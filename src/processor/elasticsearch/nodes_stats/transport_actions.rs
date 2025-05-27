use super::{ElasticsearchMetadata, NodeDocument};
use crate::processor::Metadata;
use json_patch::merge;
use rayon::prelude::*;
use serde_json::{Value, json};

/// Extract transport.actions

pub fn extract(
    actions: Value,
    metadata: &ElasticsearchMetadata,
    node_summary: Option<&NodeDocument>,
) -> Vec<Value> {
    let metadata = metadata
        .for_data_stream("metrics-node.transport.actions-esdiag")
        .as_meta_doc();

    let transport_actions: Vec<_> = match actions.as_object() {
        Some(data) => data
            .into_iter()
            .collect::<Vec<_>>()
            .par_drain(..)
            .map(|(name, action)| {
                let mut action = json!({
                    "node": node_summary,
                    "transport": {
                        "action": action,
                    },
                });

                let action_patch = json!({
                    "transport": {
                        "action": {
                            "name": name,
                        },
                    },
                });

                merge(&mut action, &action_patch);
                merge(&mut action, &metadata);
                action
            })
            .collect(),
        None => Vec::new(),
    };
    log::trace!("transport_actions: {}", transport_actions.len());
    transport_actions
}
