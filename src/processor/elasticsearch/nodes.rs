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
            let role = abbreviate_roles(node.roles.clone());
            let name = rename_node_with_role(&node.name, &role);
            let node_data = node.as_node_data(&node_id);
            lookup
                .node
                .add(node_data.rename(&name).with_role(&role))
                .with_id(&node_id)
                .with_ip(&node.ip)
                .with_host(&node.host)
                .with_name(&name);

            let patch = json!({
                "node" : {
                    "name": name,
                    "role": role,
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
            merge(&mut node_doc, &patch);
            node_doc
        })
        .collect();

    log::debug!("node settings docs: {}", nodes.len());
    nodes
}

fn rename_node_with_role(node: &String, role: &str) -> String {
    if let Some((name, number)) = node.split_once('-') {
        let number = number.trim_start_matches("000000");
        match name {
            "instance" => {
                let role_name = match role {
                    "-" => "coord",
                    "cr" => "cold",
                    "f" => "frozen",
                    "hrst" | "hirst" | "himrst" => "hot_content",
                    "i" | "ir" => "ingest",
                    "l" | "lr" => "ml",
                    "m" | "mr" => "master",
                    "mv" => "tiebreaker",
                    "w" | "wr" => "warm",
                    _ => "instance",
                };
                log::trace!("Renaming node: {}-{}", role_name, number);
                format!("{role_name}-{number}")
            }
            "tiebreaker" => format!("tiebreaker-{number}"),
            _ => node.clone(),
        }
    } else {
        node.clone()
    }
}

fn abbreviate_roles(role_list: Vec<String>) -> String {
    let char_for = |role| {
        let c = match role {
            "data" => 'd',
            "data_content" => 's',
            "data_frozen" => 'f',
            "data_hot" => 'h',
            "data_warm" => 'w',
            "data_cold" => 'c',
            "ingest" => 'i',
            "master" => 'm',
            "ml" => 'l',
            "remote_cluster_client" => 'r',
            "transform" => 't',
            _ => return None,
        };
        Some(c)
    };

    match role_list.len() {
        0 => String::from("-"),
        _ => {
            let mut roles: Vec<char> = role_list.iter().filter_map(|role| char_for(role)).collect();
            roles.sort_unstable();
            roles.iter().collect()
        }
    }
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
    role: Option<String>,
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

impl Node {
    pub fn as_node_data(&self, id: &String) -> NodeData {
        NodeData {
            attributes: self.attributes.clone(),
            host: self.host.clone(),
            id: id.clone(),
            ip: self.ip.clone(),
            name: self.name.clone(),
            os: self.os.clone(),
            role: self.role.clone().unwrap_or_default(),
            roles: self.roles.clone(),
            version: self.version.to_string(),
        }
    }
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
