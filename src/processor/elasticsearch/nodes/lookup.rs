// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{super::Lookup, Node, Nodes, OsDetails};
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_with::skip_serializing_none;
use std::collections::{HashMap, HashSet};

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

impl Lookup<NodeDocument> {
    /// Resolve a node by exact ID, falling back only to an unambiguous name.
    pub fn by_id_or_name(&self, id: &str, name: Option<&str>) -> Option<&NodeDocument> {
        self.by_id(id).or_else(|| {
            name.and_then(|name| {
                let node = self.by_name(name);
                if node.is_some() {
                    tracing::debug!(
                        "Resolved node lookup by name fallback: node_id={} node_name={}",
                        id,
                        name
                    );
                }
                node
            })
        })
    }
}

impl From<Nodes> for Lookup<NodeDocument> {
    fn from(nodes: Nodes) -> Self {
        let mut name_counts = HashMap::with_capacity(nodes.nodes.len());
        for node in nodes.nodes.values() {
            *name_counts.entry(node.name.as_str()).or_insert(0usize) += 1;
        }

        let mut lookup = Lookup::<NodeDocument>::new();
        for (id, node) in &nodes.nodes {
            lookup.add(NodeDocument::from(node).with_id(id)).with_id(id);
            if name_counts[node.name.as_str()] == 1 {
                lookup.with_name(&node.name);
            }
        }
        lookup
    }
}

impl From<Result<Nodes>> for Lookup<NodeDocument> {
    fn from(nodes_result: Result<Nodes>) -> Self {
        match nodes_result {
            Ok(nodes) => Lookup::<NodeDocument>::from_parsed(nodes),
            Err(e) => {
                tracing::warn!("Failed to parse Nodes: {}", e);
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
    } else if is_scrubbed_hex_node_name(&node_name) {
        let suffix = &node_name[node_name.len() - 4..];
        format!("{tier}-{suffix}")
    } else {
        node_name
    }
}

fn is_scrubbed_hex_node_name(node_name: &str) -> bool {
    node_name.len() == 19
        && node_name
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
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

#[cfg(test)]
mod tests {
    use super::{Lookup, NodeDocument, Nodes, get_tier_node_name};
    use serde_json::json;

    fn node_lookup(entries: &[(&str, &str, usize)]) -> Lookup<NodeDocument> {
        let mut nodes: Nodes = serde_json::from_value(json!({"_nodes": {}, "nodes": {}})).unwrap();
        for &(id, name, processors) in entries {
            let node = serde_json::from_value(json!({
                "name": name,
                "host": format!("{id}.example"),
                "build_flavor": "default",
                "build_hash": "test",
                "build_type": "tar",
                "jvm": {},
                "os": {
                    "refresh_interval_in_millis": 1000,
                    "available_processors": processors,
                    "allocated_processors": processors
                },
                "process": {},
                "roles": ["data_hot"],
                "thread_pool": {}
            }))
            .unwrap();
            nodes.nodes.insert(id.to_string(), node);
        }
        Lookup::from(nodes)
    }

    #[test]
    fn ambiguous_node_names_never_resolve_but_distinct_ids_remain_available() {
        let entries = [
            ("node-a", "shared", 2),
            ("node-b", "shared", 4),
            ("node-c", "shared", 8),
        ];
        for ordered in [entries, [entries[2], entries[1], entries[0]]] {
            let lookup = node_lookup(&ordered);
            assert!(lookup.by_name("shared").is_none());
            assert!(lookup.by_id_or_name("unknown-id", Some("shared")).is_none());
            for (id, name, processors) in entries {
                let node = lookup.by_id_or_name(id, Some(name)).unwrap();
                assert_eq!(node.id.as_deref(), Some(id));
                assert_eq!(node.host.as_deref(), Some(format!("{id}.example").as_str()));
                assert_eq!(node.os.allocated_processors, processors);
            }
        }
    }

    #[test]
    fn unique_node_name_fallback_resolves_only_the_matching_node() {
        let lookup = node_lookup(&[
            ("node-a", "shared", 2),
            ("node-b", "shared", 4),
            ("node-c", "unique", 8),
        ]);
        let node = lookup.by_id_or_name("unknown-id", Some("unique")).unwrap();
        assert_eq!(node.id.as_deref(), Some("node-c"));
        assert_eq!(node.host.as_deref(), Some("node-c.example"));
        assert_eq!(node.os.allocated_processors, 8);
        assert!(lookup.by_id_or_name("unknown-id", Some("absent")).is_none());
        assert!(lookup.by_id_or_name("unknown-id", None).is_none());
    }

    #[test]
    fn exact_node_id_takes_precedence_over_another_nodes_name() {
        let lookup = node_lookup(&[("node-a", "first", 2), ("node-b", "second", 8)]);
        let node = lookup.by_id_or_name("node-a", Some("second")).unwrap();
        assert_eq!(node.id.as_deref(), Some("node-a"));
        assert_eq!(node.host.as_deref(), Some("node-a.example"));
        assert_eq!(node.os.allocated_processors, 2);
    }

    #[test]
    fn preserves_existing_instance_rename_behavior() {
        assert_eq!(get_tier_node_name("instance-0000000001".to_string(), "hot"), "hot-0001");
    }

    #[test]
    fn humanizes_scrubbed_hex_name_with_last_four_chars() {
        assert_eq!(get_tier_node_name("aaaabbbbccccddddee0".to_string(), "hot"), "hot-dee0");
    }
}
