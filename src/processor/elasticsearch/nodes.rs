use super::lookup::node::NodeData;
use super::metadata::{DataStream, Metadata, MetadataDoc};
use json_patch::merge;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn enrich_lookup(metadata: &mut Metadata, data: String) -> Vec<Value> {
    let lookup = &mut metadata.lookup;
    let nodes_data: Nodes = match serde_json::from_str(&data) {
        Ok(data) => data,
        Err(e) => {
            log::warn!("Failed to deserialize nodes: {}", e);
            return Vec::new();
        }
    };

    let node_doc = NodeDoc::new(
        metadata.as_doc.clone(),
        DataStream::from("settings-node-esdiag"),
    );

    log::debug!("nodes: {}", nodes_data.nodes.len());

    let nodes: Vec<Value> = nodes_data
        .nodes
        .into_iter()
        .map(|(node_id, node)| {
            let node_data = NodeData {
                attributes: node.attributes.clone(),
                host: node.host.clone(),
                id: node_id.clone(),
                ip: node.ip.clone(),
                name: node.name.clone(),
                roles: node.roles.clone(),
                version: node.version.to_string(),
            };
            lookup
                .node
                .add(node_data)
                .with_id(&node_id)
                .with_ip(&node.ip)
                .with_host(&node.host)
                .with_name(&node.name);

            // Remove nested field names that cause mapping issues
            let omit = json!({
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
            let mut node_doc = json!(node_doc.clone().with(node));
            merge(&mut node_doc, &omit);
            node_doc
        })
        .collect();

    log::debug!("node settings docs: {}", nodes.len());
    nodes
}

// Serializing data structures

#[derive(Clone, Serialize)]
struct NodeDoc {
    #[serde(flatten)]
    metadata: MetadataDoc,
    data_stream: DataStream,
    node: Option<Node>,
}

impl NodeDoc {
    pub fn new(metadata: MetadataDoc, data_stream: DataStream) -> Self {
        NodeDoc {
            data_stream,
            metadata,
            node: None,
        }
    }

    pub fn with(mut self, node: Node) -> Self {
        self.node = Some(node);
        self
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Node {
    aggregations: Value,
    attributes: Value,
    build_flavor: String,
    build_hash: String,
    build_type: String,
    component_version: Option<ComponentVersion>,
    host: String,
    http: Value,
    index_version: Option<i64>,
    ingest: Value,
    ip: String,
    jvm: Value,
    modules: Value,
    name: String,
    os: Value,
    plugins: Value,
    process: Value,
    roles: Vec<String>,
    settings: Value,
    thread_pool: Value,
    total_indexing_buffer: Value,
    total_indexing_buffer_in_bytes: Value,
    transport: Value,
    transport_address: String,
    transport_version: Option<i64>,
    version: semver::Version,
}

#[derive(Clone, Deserialize, Serialize)]
struct ComponentVersion {
    ml_config_version: i64,
    transform_config_version: i64,
}

// Deserializing data structures

#[derive(Deserialize)]
struct Nodes {
    _nodes: Value,
    //cluster_name: String,
    nodes: HashMap<String, Node>,
}
