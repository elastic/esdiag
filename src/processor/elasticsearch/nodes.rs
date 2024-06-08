use crate::processor::elasticsearch::lookup::Identifiers;

use super::lookup::node::NodeData;
use super::metadata::Metadata;
use json_patch::merge;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
struct Nodes {
    _nodes: Value,
    cluster_name: String,
    nodes: HashMap<String, Node>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ComponentVersion {
    ml_config_version: i64,
    transform_config_version: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Node {
    name: String,
    transport_address: String,
    host: String,
    ip: String,
    version: semver::Version,
    transport_version: Option<i64>,
    index_version: Option<i64>,
    component_version: Option<ComponentVersion>,
    build_flavor: String,
    build_type: String,
    build_hash: String,
    total_indexing_buffer_in_bytes: Value,
    total_indexing_buffer: Value,
    roles: Vec<String>,
    attributes: Value,
    settings: Value,
    os: Value,
    process: Value,
    jvm: Value,
    thread_pool: Value,
    transport: Value,
    http: Value,
    plugins: Value,
    modules: Value,
    ingest: Value,
    aggregations: Value,
}

pub async fn enrich_lookup(metadata: &mut Metadata, data: String) -> Vec<Value> {
    let nodes_data: Nodes = match serde_json::from_str(&data) {
        Ok(data) => data,
        Err(e) => {
            log::warn!("Failed to deserialize nodes: {}", e);
            return Vec::<Value>::new();
        }
    };

    log::debug!("nodes: {}", nodes_data.nodes.len());

    let data_stream = json!({
        "data_stream": {
            "dataset": "node",
            "namespace": "esdiag",
            "type": "settings",
        }
    });

    let mut nodes = Vec::new();
    for (node_id, node) in nodes_data.nodes {
        metadata.lookup.node.insert(
            Identifiers {
                id: Some(node_id.clone()),
                name: Some(node.name.clone()),
                host: Some(node.host.clone()),
                ip: Some(node.ip.clone()),
            },
            NodeData {
                attributes: node.attributes.clone(),
                host: node.host.clone(),
                id: node_id.clone(),
                ip: node.ip.clone(),
                name: node.name.clone(),
                roles: node.roles.clone(),
                version: node.version.to_string(),
            },
        );
        let mut doc = json!({
            "@timestamp": metadata.diagnostic.collection_date,
            "node": metadata.lookup.node.by_id(&node_id),
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

        merge(&mut doc, &json!(node));
        merge(&mut doc, &omit);
        merge(&mut doc, &data_stream);
        nodes.push(doc);
    }

    log::debug!("node settings docs: {}", nodes.len());
    nodes
}
