use super::metadata::Metadata;
use json_patch::merge;
use serde_json::{json, Value};

pub async fn enrich_lookup(metadata: &mut Metadata, data: Value) -> Vec<Value> {
    let nodes_data: Vec<_> = match data["nodes"].as_object() {
        Some(data) => data.into_iter().collect(),
        None => return Vec::new(),
    };
    log::debug!("nodes: {}", nodes_data.len());

    let data_stream = json!({
        "data_stream": {
            "dataset": "node",
            "namespace": "esdiag",
            "type": "settings",
        }
    });

    let mut nodes = Vec::new();
    for (node_id, node) in &nodes_data {
        metadata.lookup.node.insert(&node_id, &node);
        let mut doc = json!({
            "@timestamp": metadata.diagnostic.collection_date,
            "node": metadata.lookup.node.by_id(node_id.as_str()),
            "cluster": metadata.cluster,
            "diagnostic": metadata.diagnostic,
        });

        let omit = json!({
            // Remove duplicate fields from metadata
            "attributes": null,
            "build_flavor": null,
            "build_hash": null,
            "build_type": null,
            "host": null,
            "ip": null,
            "name": null,
            "roles": null,
            "version": null,
            // Remove nested field names that cause mapping issues
            "settings": {
                "http": {
                    "type.default": null,
                },
                "transport": {
                    "type.default": null,
                },
            }
        });

        merge(&mut doc, &node);
        merge(&mut doc, &omit);
        merge(&mut doc, &data_stream);
        nodes.push(doc);
    }

    log::debug!("node settings docs: {}", nodes.len());
    nodes
}
