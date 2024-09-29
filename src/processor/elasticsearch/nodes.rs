use super::{
    lookup::node::NodeData,
    metadata::{DataStreamName, Metadata, MetadataDoc},
};
use crate::data::elasticsearch::{Node, Nodes};
use json_patch::merge;
use serde::Serialize;
use serde_json::{json, Value};

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
        DataStreamName::from("settings-node-esdiag"),
    );

    log::debug!("nodes: {}", nodes_data.nodes.len());

    let nodes: Vec<Value> = nodes_data
        .nodes
        .into_iter()
        .map(|(node_id, node)| {
            let role = abbreviate_roles(node.roles.clone());
            let name = rename_node_with_role(&node.name, &role);
            let node_data = NodeData::from(&node).with_id(&node_id);
            lookup
                .node
                .add(node_data.rename(&name).with_role(&role))
                .with_id(&node_id)
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
    data_stream: DataStreamName,
    node: Option<Node>,
}

impl NodeDoc {
    pub fn new(metadata: MetadataDoc, data_stream: DataStreamName) -> Self {
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
