use super::Lookup;
use crate::data::elasticsearch::{Node, Nodes};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Serialize)]
pub struct NodeData {
    pub attributes: Value,
    pub host: String,
    pub id: Option<String>,
    pub ip: String,
    pub name: String,
    pub os: Value,
    pub role: String,
    pub roles: Vec<String>,
    pub version: String,
}

impl NodeData {
    pub fn rename(self, name: &String) -> Self {
        NodeData {
            name: name.clone(),
            ..self
        }
    }

    pub fn with_id(self, id: &String) -> Self {
        NodeData {
            id: Some(id.clone()),
            ..self
        }
    }

    pub fn with_role(self, role: &String) -> Self {
        NodeData {
            role: role.clone(),
            ..self
        }
    }
}

impl From<&Node> for NodeData {
    fn from(node: &Node) -> Self {
        NodeData {
            attributes: node.attributes.clone(),
            host: node.host.clone(),
            id: None,
            ip: node.ip.clone(),
            name: node.name.clone(),
            os: node.os.clone(),
            role: node.role.clone().unwrap_or_default(),
            roles: node.roles.clone(),
            version: node.version.to_string(),
        }
    }
}

impl From<Nodes> for Lookup<Node> {
    fn from(mut nodes: Nodes) -> Self {
        let mut lookup = Lookup::<Node>::new();
        nodes.nodes.drain().for_each(|(id, node)| {
            let name = node.name.clone();
            lookup.add(node).with_name(&name).with_id(&id);
        });
        lookup
    }
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap())
    }
}
