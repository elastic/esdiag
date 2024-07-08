use super::LookupDisplay;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Serialize)]
pub struct NodeData {
    pub attributes: Value,
    pub host: String,
    pub id: String,
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

    pub fn with_role(self, role: &String) -> Self {
        NodeData {
            role: role.clone(),
            ..self
        }
    }
}

impl LookupDisplay for NodeData {
    fn display() -> &'static str {
        "node_data"
    }
}
