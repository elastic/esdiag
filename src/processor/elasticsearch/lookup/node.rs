use crate::{
    data::elasticsearch::{Node, Nodes, OsDetails},
    processor::lookup::Lookup,
};
use color_eyre::eyre::Result;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Serialize)]
pub struct NodeSummary {
    pub attributes: Option<Value>,
    pub host: Option<String>,
    pub id: Option<String>,
    pub ip: Option<String>,
    pub name: String,
    pub os: OsDetails,
    pub role: String,
    pub roles: Vec<String>,
    pub tier: String,
    pub tier_order: usize,
    pub version: Option<String>,
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
}

impl From<&Node> for NodeSummary {
    fn from(node: &Node) -> Self {
        let role = get_roles_abbreviation(&node.roles);
        let tier = get_tier(&node.roles);
        let tier_order = get_tier_order(&tier);
        let name = get_tier_node_name(node.name.clone(), &tier);

        NodeSummary {
            attributes: node.attributes.clone(),
            host: node.host.clone(),
            id: None,
            ip: node.ip.clone(),
            name,
            os: node.os.clone(),
            role,
            roles: node.roles.clone(),
            tier,
            tier_order,
            version: node.version.as_ref().map(|v| v.to_string()),
        }
    }
}

impl From<Nodes> for Lookup<NodeSummary> {
    fn from(mut nodes: Nodes) -> Self {
        let mut lookup = Lookup::<NodeSummary>::new();
        nodes.nodes.drain().for_each(|(id, node)| {
            lookup
                .add(NodeSummary::from(&node).with_id(&id))
                .with_name(&node.name)
                .with_id(&id);
        });
        lookup
    }
}

impl From<Result<Nodes>> for Lookup<NodeSummary> {
    fn from(nodes_result: Result<Nodes>) -> Self {
        match nodes_result {
            Ok(nodes) => Lookup::<NodeSummary>::from(nodes),
            Err(e) => {
                log::warn!("Failed to parse Nodes: {}", e);
                Lookup::new()
            }
        }
    }
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap())
    }
}

/// Determines a node's tier based on a precedence of assigned roles.
fn get_tier(roles: &Vec<String>) -> String {
    match () {
        _ if roles.contains(&"data_hot".to_string()) => "hot",
        _ if roles.contains(&"data_warm".to_string()) => "warm",
        _ if roles.contains(&"data_cold".to_string()) => "cold",
        _ if roles.contains(&"data_frozen".to_string()) => "frozen",
        _ if roles.contains(&"data_content".to_string()) => "content",
        _ if roles.contains(&"data".to_string()) => "data",
        _ if roles.contains(&"ingest".to_string()) => "ingest",
        _ if roles.contains(&"ml".to_string()) => "ml",
        _ if roles.contains(&"transform".to_string()) => "transform",
        _ if roles.contains(&"voting_only".to_string()) => "tiebreaker",
        _ if roles.contains(&"master".to_string()) => "master",
        _ if roles.contains(&"remote_cluster_client".to_string()) => "remote",
        _ if roles.is_empty() => "coord",
        _ => "node",
    }
    .to_string()
}

/// Return a number for tier sorting
fn get_tier_order(tier: &str) -> usize {
    match tier {
        "hot" => 0,
        "warm" => 1,
        "cold" => 2,
        "frozen" => 3,
        "content" => 4,
        "data" => 5,
        "ingest" => 6,
        "ml" => 7,
        "transform" => 8,
        "tiebreaker" => 9,
        "master" => 10,
        "remote" => 11,
        "coord" => 12,
        "node" => 13,
        _ => 99,
    }
}

/// Renames an `instance-0000000001` type name into a `tier-00001` name.
fn get_tier_node_name(node_name: String, tier: &str) -> String {
    if let Some(("instance", number)) = node_name.split_once('-') {
        let number = number.trim_start_matches("000000");
        format!("{}-{}", tier, number)
    } else {
        node_name
    }
}

/// Collects single-character abbreviations for roles into a string.
fn get_roles_abbreviation(role_list: &Vec<String>) -> String {
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
