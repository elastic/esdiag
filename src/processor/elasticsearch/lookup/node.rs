use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NodeData {
    attributes: Value,
    host: String,
    id: String,
    ip: String,
    name: String,
    roles: Value,
    version: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NodeLookup {
    pub nodes: Vec<NodeData>,
    pub by_id: HashMap<String, usize>,
    pub by_name: HashMap<String, usize>,
    pub by_host: HashMap<String, usize>,
    pub by_ip: HashMap<String, usize>,
}

impl NodeLookup {
    pub fn new() -> NodeLookup {
        NodeLookup {
            nodes: Vec::new(),
            by_id: HashMap::new(),
            by_name: HashMap::new(),
            by_host: HashMap::new(),
            by_ip: HashMap::new(),
        }
    }

    pub fn from_value(nodes: Value) -> NodeLookup {
        let mut node_lookup = NodeLookup::new();

        for (id, data) in nodes["nodes"].as_object().unwrap() {
            let node = NodeData {
                attributes: data["attributes"].clone(),
                host: data["host"].as_str().unwrap().to_string(),
                id: id.clone(),
                ip: data["ip"].as_str().unwrap().to_string(),
                name: data["name"].as_str().unwrap().to_string(),
                roles: data["roles"].clone(),
                version: data["version"].as_str().unwrap().to_string(),
            };
            let index = node_lookup.nodes.len();
            node_lookup.by_id.insert(node.id.to_string(), index);
            node_lookup.by_name.insert(node.name.clone(), index);
            node_lookup.by_host.insert(node.host.clone(), index);
            node_lookup.by_ip.insert(node.ip.clone(), index);
            node_lookup.nodes.push(node);
        }
        node_lookup
    }
}
