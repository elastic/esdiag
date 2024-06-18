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
    pub roles: Vec<String>,
    pub version: String,
}

impl LookupDisplay for NodeData {
    fn display() -> &'static str {
        "node_data"
    }
}
