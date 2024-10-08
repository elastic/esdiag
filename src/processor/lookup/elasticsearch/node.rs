use super::Lookup;
use crate::{
    data::elasticsearch::{Node, Nodes},
    processor::lookup::LookupTable,
};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Serialize)]
pub struct NodeSummary {
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

impl NodeSummary {
    pub fn rename(self, name: &String) -> Self {
        NodeSummary {
            name: name.clone(),
            ..self
        }
    }

    pub fn with_id(self, id: &String) -> Self {
        NodeSummary {
            id: Some(id.clone()),
            ..self
        }
    }

    pub fn with_role(self, role: &String) -> Self {
        NodeSummary {
            role: role.clone(),
            ..self
        }
    }
}

impl LookupTable for Lookup<NodeSummary> {}

impl From<&Node> for NodeSummary {
    fn from(node: &Node) -> Self {
        NodeSummary {
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

impl From<Nodes> for Lookup<NodeSummary> {
    fn from(mut nodes: Nodes) -> Self {
        let mut lookup = Lookup::<NodeSummary>::new();
        nodes.nodes.drain().for_each(|(id, node)| {
            let role = abbreviate_roles(&node.roles);
            let name = rename_with_role(&node.name, &role);
            let node_summary = NodeSummary::from(&node)
                .with_id(&id)
                .with_role(&role)
                .rename(&name);
            let name = node.name.clone();
            lookup.add(node_summary).with_name(&name).with_id(&id);
        });
        lookup
    }
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap())
    }
}

fn rename_with_role(node_name: &String, role: &str) -> String {
    if let Some((name, number)) = node_name.split_once('-') {
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
                    "w" | "rw" => "warm",
                    _ => "instance",
                };
                log::trace!("Renaming node: {}-{}", role_name, number);
                format!("{role_name}-{number}")
            }
            "tiebreaker" => format!("tiebreaker-{number}"),
            _ => node_name.clone(),
        }
    } else {
        node_name.clone()
    }
}

fn abbreviate_roles(role_list: &Vec<String>) -> String {
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
