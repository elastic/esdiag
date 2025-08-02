use super::super::super::nodes::NodeDocument;
use super::{ElasticsearchMetadata, Metadata};
use json_patch::merge;
use rayon::prelude::*;
use serde_json::{Value, json};

/// Extract http.clients
pub fn extract(
    clients: Value,
    metadata: &ElasticsearchMetadata,
    node_summary: Option<&NodeDocument>,
) -> Vec<Value> {
    let metadata = metadata
        .for_data_stream("metrics-node.http.clients-esdiag")
        .as_meta_doc();

    let clients: Vec<_> = match clients.as_array() {
        Some(data) => data
            .into_iter()
            .collect::<Vec<_>>()
            .par_drain(..)
            .map(|client| {
                let mut doc = json!({ "node": node_summary, "http": { "client": client, }});
                merge(&mut doc, &metadata);
                doc
            })
            .collect(),
        None => Vec::new(),
    };
    log::trace!("clients: {}", clients.len());
    clients
}
