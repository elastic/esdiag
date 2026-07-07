//! Shared assertions for scrubbed IP normalization integration tests.
//! Validates deterministic transform, valid IPv4 octets, and stable node-id → IP mapping
//! across exported NDJSON streams.

use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

/// NDJSON exports that embed `node.id` with `node.host` / `node.ip` from node lookup.
const NODE_IP_STREAMS: &[&str] = &[
    "metrics-node-esdiag.ndjson",
    "metrics-task-esdiag.ndjson",
    "metrics-node.transport.actions-esdiag.ndjson",
    "metrics-node.http.clients-esdiag.ndjson",
    "metrics-node.discovery.cluster_applier-esdiag.ndjson",
    "metrics-node.discovery.cluster_adaptive-esdiag.ndjson",
    "metrics-ingest.pipeline-esdiag.ndjson",
    "metrics-ingest.processor-esdiag.ndjson",
    "metrics-shard-esdiag.ndjson",
    "settings-node-esdiag.ndjson",
];

pub const SECOND_NODE_ID: &str = "syntheticSecondNode0123456789ab";

#[derive(Clone, Debug)]
pub struct ScrubFixtureExpectations {
    pub normalized_by_node_id: HashMap<String, String>,
    malformed_by_node_id: HashMap<String, String>,
}

impl ScrubFixtureExpectations {
    pub fn malformed_ips(&self) -> impl Iterator<Item = &String> {
        self.malformed_by_node_id.values()
    }

    pub fn node_count(&self) -> usize {
        self.normalized_by_node_id.len()
    }
}

pub fn ensure_two_nodes_in_nodes_json(content: &str) -> String {
    duplicate_nodes_object(content, "nodes")
}

pub fn ensure_two_nodes_in_nodes_stats_json(content: &str) -> String {
    duplicate_nodes_object(content, "nodes")
}

pub fn ensure_two_nodes_in_tasks_json(content: &str) -> String {
    duplicate_nodes_object(content, "nodes")
}

fn duplicate_nodes_object(content: &str, nodes_key: &str) -> String {
    let mut value: Value = serde_json::from_str(content).expect("parse nodes object json");
    let Some(nodes) = value.get_mut(nodes_key).and_then(Value::as_object_mut) else {
        return content.to_string();
    };
    if nodes.len() >= 2 {
        return serde_json::to_string(&value).expect("serialize nodes object json");
    }

    let first_node = nodes.values().next().expect("fixture node").clone();
    nodes.insert(SECOND_NODE_ID.to_string(), first_node);
    serde_json::to_string(&value).expect("serialize nodes object json")
}

pub fn malformed_ip_for_index(index: usize) -> String {
    format!("512.768.{}.{}", 1024 + index, 1280 + index)
}

pub fn expected_normalized_ip(value: &str) -> String {
    value
        .split('.')
        .map(|octet| octet.parse::<u16>().unwrap_or(0) % 255)
        .map(|octet| octet.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

pub fn strip_port(value: &str) -> &str {
    let Some((ip, port)) = value.rsplit_once(':') else {
        return value;
    };
    if ip.contains(':') || !port.chars().all(|c| c.is_ascii_digit()) {
        return value;
    }
    ip
}

pub fn is_valid_ipv4(value: &str) -> bool {
    let ip = strip_port(value);
    let octets: Vec<u16> = ip.split('.').filter_map(|part| part.parse().ok()).collect();
    octets.len() == 4 && octets.iter().all(|octet| *octet <= 255)
}

pub fn inject_malformed_ips_in_nodes_json(content: &str) -> (String, ScrubFixtureExpectations) {
    let mut value: Value = serde_json::from_str(content).expect("parse nodes.json");
    let mut normalized_by_node_id = HashMap::new();
    let mut malformed_by_node_id = HashMap::new();

    if let Some(nodes) = value.get_mut("nodes").and_then(Value::as_object_mut) {
        let mut node_ids: Vec<_> = nodes.keys().cloned().collect();
        node_ids.sort();

        for (index, node_id) in node_ids.iter().enumerate() {
            let malformed = malformed_ip_for_index(index);
            let normalized = expected_normalized_ip(&malformed);
            let node = nodes.get_mut(node_id).expect("node id from sorted keys");
            if let Some(obj) = node.as_object_mut() {
                obj.insert("ip".into(), Value::String(malformed.clone()));
                obj.insert("host".into(), Value::String(malformed.clone()));
                obj.insert(
                    "transport_address".into(),
                    Value::String(format!("{malformed}:9300")),
                );
                if let Some(transport) = obj.get_mut("transport").and_then(Value::as_object_mut) {
                    transport.insert(
                        "publish_address".into(),
                        Value::String(format!("{malformed}:9300")),
                    );
                }
                let settings = obj
                    .entry("settings")
                    .or_insert_with(|| Value::Object(Default::default()));
                if let Some(settings) = settings.as_object_mut() {
                    let network_settings = settings
                        .entry("network")
                        .or_insert_with(|| Value::Object(Default::default()));
                    if let Some(net) = network_settings.as_object_mut() {
                        net.insert(
                            "publish_host".into(),
                            Value::String(malformed.clone()),
                        );
                    }
                }
            }
            normalized_by_node_id.insert(node_id.clone(), normalized);
            malformed_by_node_id.insert(node_id.clone(), malformed);
        }
    }

    let expectations = ScrubFixtureExpectations {
        normalized_by_node_id,
        malformed_by_node_id,
    };
    (
        serde_json::to_string(&value).expect("serialize nodes.json"),
        expectations,
    )
}

pub fn inject_malformed_ips_in_nodes_stats_json(
    content: &str,
    expectations: &ScrubFixtureExpectations,
) -> String {
    let mut value: Value = serde_json::from_str(content).expect("parse nodes_stats.json");

    if let Some(nodes) = value.get_mut("nodes").and_then(Value::as_object_mut) {
        for (node_id, node) in nodes.iter_mut() {
            let Some(malformed) = expectations.malformed_by_node_id.get(node_id) else {
                continue;
            };
            if let Some(obj) = node.as_object_mut() {
                obj.insert("ip".into(), Value::String(format!("{malformed}:9300")));
                obj.insert("host".into(), Value::String(malformed.clone()));
                obj.insert(
                    "transport_address".into(),
                    Value::String(format!("{malformed}:9300")),
                );
            }
        }
    }

    serde_json::to_string(&value).expect("serialize nodes_stats.json")
}

pub fn inject_malformed_ips_in_tasks_json(
    content: &str,
    expectations: &ScrubFixtureExpectations,
) -> String {
    let mut value: Value = serde_json::from_str(content).expect("parse tasks.json");

    if let Some(nodes) = value.get_mut("nodes").and_then(Value::as_object_mut) {
        for (node_id, node) in nodes.iter_mut() {
            let Some(malformed) = expectations.malformed_by_node_id.get(node_id) else {
                continue;
            };
            if let Some(obj) = node.as_object_mut() {
                obj.insert("ip".into(), Value::String(format!("{malformed}:9300")));
                obj.insert("host".into(), Value::String(malformed.clone()));
                obj.insert(
                    "transport_address".into(),
                    Value::String(format!("{malformed}:9300")),
                );
            }
        }
    }

    serde_json::to_string(&value).expect("serialize tasks.json")
}

pub fn assert_scrubbed_export(
    output_dir: &Path,
    expectations: &ScrubFixtureExpectations,
) {
    let mut observed_by_node: HashMap<String, HashSet<String>> = HashMap::new();

    for stream in NODE_IP_STREAMS {
        let path = output_dir.join(stream);
        if !path.exists() {
            continue;
        }

        let content = fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!("read {}: {err}", path.display());
        });

        for (line_no, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let doc: Value = serde_json::from_str(line).unwrap_or_else(|err| {
                panic!("parse {stream} line {}: {err}", line_no + 1);
            });
            let Some(node) = doc.get("node").and_then(Value::as_object) else {
                continue;
            };
            let Some(node_id) = node.get("id").and_then(Value::as_str) else {
                continue;
            };
            let Some(expected_ip) = expectations.normalized_by_node_id.get(node_id) else {
                continue;
            };

            for field in ["host", "ip", "transport_address"] {
                let Some(raw) = node.get(field).and_then(Value::as_str) else {
                    continue;
                };
                assert_normalized_address_field(
                    stream,
                    line_no + 1,
                    &format!("node.{field}"),
                    raw,
                    expected_ip,
                    expectations,
                );

                observed_by_node
                    .entry(node_id.to_string())
                    .or_default()
                    .insert(strip_port(raw).to_string());
            }

            if *stream == "settings-node-esdiag.ndjson" {
                if let Some(publish_host) = node
                    .get("settings")
                    .and_then(|settings| settings.get("network"))
                    .and_then(|network| network.get("publish_host"))
                    .and_then(Value::as_str)
                    && looks_like_dotted_quad(publish_host)
                {
                    assert_normalized_address_field(
                        stream,
                        line_no + 1,
                        "node.settings.network.publish_host",
                        publish_host,
                        expected_ip,
                        expectations,
                    );
                }
            }
        }
    }

    assert!(
        expectations.node_count() >= 2,
        "fixture must include at least two nodes to validate mapping isolation"
    );
    let distinct_expected: HashSet<_> = expectations.normalized_by_node_id.values().collect();
    assert_eq!(
        distinct_expected.len(),
        expectations.node_count(),
        "each node must have a distinct normalized IP in the fixture"
    );

    for (node_id, expected_ip) in &expectations.normalized_by_node_id {
        let seen = observed_by_node.get(node_id).cloned().unwrap_or_default();
        assert!(
            seen.contains(expected_ip),
            "expected normalized IP {expected_ip} for node_id={node_id} in at least one export stream, saw {seen:?}"
        );
        assert_eq!(
            seen.len(),
            1,
            "node_id={node_id} should map to exactly one normalized IP across streams, saw {seen:?}"
        );
    }
}

fn looks_like_dotted_quad(value: &str) -> bool {
    let ip = strip_port(value);
    ip.split('.').count() == 4
        && ip
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

fn assert_normalized_address_field(
    stream: &str,
    line_no: usize,
    field_path: &str,
    raw: &str,
    expected_ip: &str,
    expectations: &ScrubFixtureExpectations,
) {
    let normalized_field = strip_port(raw).to_string();
    assert!(
        is_valid_ipv4(raw),
        "{stream} line {line_no} {field_path}={raw:?} is not valid IPv4"
    );
    for malformed in expectations.malformed_ips() {
        assert!(
            !raw.contains(malformed),
            "{stream} line {line_no} {field_path} still contains malformed IP {malformed}"
        );
    }
    assert_eq!(
        &normalized_field, expected_ip,
        "{stream} line {line_no} {field_path} mismatch"
    );
}

pub fn assert_malformed_ips_preserved_in_node_metrics(output_dir: &Path, malformed: &str) {
    let path = output_dir.join("metrics-node-esdiag.ndjson");
    assert!(path.exists(), "expected {}", path.display());
    let content = fs::read_to_string(&path).expect("read node metrics");
    assert!(
        content.contains(malformed),
        "expected malformed IP {malformed} to remain when scrub is disabled"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_malformed_ips_normalize_to_distinct_valid_ips() {
        let mut normalized = HashSet::new();
        for index in 0..4 {
            let malformed = malformed_ip_for_index(index);
            let fixed = expected_normalized_ip(&malformed);
            assert!(is_valid_ipv4(&fixed));
            assert!(normalized.insert(fixed));
        }
    }
}
