// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{super::Lookup, Node, Nodes, OsDetails};
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_with::skip_serializing_none;
use std::collections::HashSet;

#[skip_serializing_none]
#[derive(Clone, Deserialize, Serialize)]
pub struct NodeDocument {
    pub attributes: Option<Box<RawValue>>,
    pub host: Option<String>,
    pub id: Option<String>,
    pub ip: Option<String>,
    pub name: String,
    pub os: OsDetails,
    pub role: String,
    pub roles: HashSet<String>,
    pub tier: String,
    pub tier_order: usize,
    pub version: Option<String>,
}

impl NodeDocument {
    pub fn rename(self, name: &str) -> Self {
        NodeDocument {
            name: name.to_string(),
            ..self
        }
    }

    pub fn with_id(self, id: &str) -> Self {
        NodeDocument {
            id: Some(id.to_string()),
            ..self
        }
    }
}

impl From<&Node> for NodeDocument {
    fn from(node: &Node) -> Self {
        let role = get_roles_abbreviation(&node.roles);
        let tier = get_tier(&node.roles);
        let tier_order = get_tier_order(&tier);
        let name = get_tier_node_name(node.name.clone(), &tier);

        NodeDocument {
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

impl From<Nodes> for Lookup<NodeDocument> {
    fn from(mut nodes: Nodes) -> Self {
        let mut lookup = Lookup::<NodeDocument>::new();
        nodes.nodes.drain().for_each(|(id, node)| {
            lookup
                .add(NodeDocument::from(&node).with_id(&id))
                .with_name(&node.name)
                .with_id(&id);
        });
        lookup
    }
}

impl From<Result<Nodes>> for Lookup<NodeDocument> {
    fn from(nodes_result: Result<Nodes>) -> Self {
        match nodes_result {
            Ok(nodes) => Lookup::<NodeDocument>::from_parsed(nodes),
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
fn get_tier(roles: &HashSet<String>) -> String {
    match () {
        _ if roles.contains("index") => "index",
        _ if roles.contains("search") => "search",
        _ if roles.contains("data_hot") => "hot",
        _ if roles.contains("data_warm") => "warm",
        _ if roles.contains("data_cold") => "cold",
        _ if roles.contains("data_frozen") => "frozen",
        _ if roles.contains("data_content") => "content",
        _ if roles.contains("data") => "data",
        _ if roles.contains("ingest") => "ingest",
        _ if roles.contains("ml") => "ml",
        _ if roles.contains("transform") => "transform",
        _ if roles.contains("voting_only") => "tiebreaker",
        _ if roles.contains("master") => "master",
        _ if roles.contains("remote_cluster_client") => "remote",
        _ if roles.is_empty() => "coord",
        _ => "node",
    }
    .to_string()
}

/// Return a number for tier sorting
fn get_tier_order(tier: &str) -> usize {
    match tier {
        "index" => 0,
        "search" => 1,
        "hot" => 2,
        "warm" => 3,
        "cold" => 4,
        "frozen" => 5,
        "content" => 6,
        "data" => 7,
        "ingest" => 8,
        "ml" => 9,
        "transform" => 10,
        "tiebreaker" => 11,
        "master" => 12,
        "remote" => 13,
        "coord" => 14,
        "node" => 15,
        _ => 99,
    }
}

/// Renames default Elastic Cloud names into something more compact.
fn get_tier_node_name(node_name: String, tier: &str) -> String {
    if let Some(("instance", number)) = node_name.split_once('-') {
        // Renames `instance-0000000001` into `tier-00001`
        let number = number.trim_start_matches("000000");
        format!("{}-{}", tier, number)
    } else {
        node_name
    }
}

/// Collects single-character abbreviations for roles into a string.
fn get_roles_abbreviation(role_list: &HashSet<String>) -> String {
    let char_for = |role: &str| {
        let c = match role {
            "data" => 'd',
            "data_content" => 's',
            "data_frozen" => 'f',
            "data_hot" => 'h',
            "data_warm" => 'w',
            "data_cold" => 'c',
            "index" => 'I',
            "ingest" => 'i',
            "master" => 'm',
            "ml" => 'l',
            "remote_cluster_client" => 'r',
            "search" => 'S',
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
