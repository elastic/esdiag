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

impl NodeData {
    pub fn from_value(id: &String, node: &Value) -> NodeData {
        NodeData {
            attributes: node["attributes"].clone(),
            host: node["host"].as_str().unwrap().to_string(),
            id: id.clone(),
            ip: node["ip"].as_str().unwrap().to_string(),
            name: node["name"].as_str().unwrap().to_string(),
            roles: node["roles"].clone(),
            version: node["version"].as_str().unwrap().to_string(),
        }
    }
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

        for (id, data) in nodes["nodes"].as_object().expect("Failed to get nodes") {
            node_lookup.insert(id, data);
        }
        node_lookup
    }

    pub fn insert(&mut self, id: &String, node: &Value) {
        let node = NodeData::from_value(id, node);

        let index = self.nodes.len();
        self.by_id.insert(node.id.clone(), index);
        self.by_name.insert(node.name.clone(), index);
        self.by_host.insert(node.host.clone(), index);
        self.by_ip.insert(node.ip.clone(), index);
        self.nodes.push(node);
    }
}
