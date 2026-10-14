// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Label and pod template helpers that decide which Kubernetes values are
//! safe and useful to index.

use std::collections::BTreeMap;

pub const OPERATOR_LABEL: (&str, &str) = ("control-plane", "elastic-operator");
pub const ZONE_LABEL: &str = "topology.kubernetes.io/zone";
pub const NODE_ROLE_PREFIX: &str = "node-role.kubernetes.io/";
pub const TYPE_LABEL: &str = "common.k8s.elastic.co/type";
const ES_NODE_ROLE_PREFIX: &str = "elasticsearch.k8s.elastic.co/node-";

/// Labels keyed by an Elastic-owned or Kubernetes topology key. User labels
/// may carry arbitrary text, so everything else is dropped.
pub fn is_indexed_label(key: &str) -> bool {
    let (prefix, _) = key.split_once('/').unwrap_or(("", key));
    prefix.ends_with(".k8s.elastic.co")
        || key == OPERATOR_LABEL.0
        || key == "statefulset.kubernetes.io/pod-name"
        || key == ZONE_LABEL
        || key.starts_with(NODE_ROLE_PREFIX)
}

pub fn indexed_labels(labels: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    labels
        .iter()
        .filter(|(key, _)| is_indexed_label(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

pub fn is_operator(labels: &BTreeMap<String, String>) -> bool {
    labels.get(OPERATOR_LABEL.0).map(String::as_str) == Some(OPERATOR_LABEL.1)
}

/// Kubernetes node roles from `node-role.kubernetes.io/<role>` labels.
pub fn node_roles(labels: &BTreeMap<String, String>) -> Vec<String> {
    labels
        .keys()
        .filter_map(|key| key.strip_prefix(NODE_ROLE_PREFIX))
        .filter(|role| !role.is_empty())
        .map(str::to_string)
        .collect()
}

/// Elasticsearch roles from `elasticsearch.k8s.elastic.co/node-<role>: "true"`.
pub fn elasticsearch_roles(labels: &BTreeMap<String, String>) -> Vec<String> {
    labels
        .iter()
        .filter(|(_, value)| value.as_str() == "true")
        .filter_map(|(key, _)| key.strip_prefix(ES_NODE_ROLE_PREFIX))
        .map(str::to_string)
        .collect()
}

/// The ECK resource that owns a pod, from the operator's name labels.
pub fn owning_resource(labels: &BTreeMap<String, String>) -> Option<(&'static str, String)> {
    const OWNER_LABELS: [(&str, &str); 8] = [
        ("elasticsearch.k8s.elastic.co/cluster-name", "Elasticsearch"),
        ("kibana.k8s.elastic.co/name", "Kibana"),
        ("agent.k8s.elastic.co/name", "Agent"),
        ("logstash.k8s.elastic.co/name", "Logstash"),
        ("beat.k8s.elastic.co/name", "Beat"),
        ("apm.k8s.elastic.co/name", "ApmServer"),
        ("enterprisesearch.k8s.elastic.co/name", "EnterpriseSearch"),
        ("maps.k8s.elastic.co/name", "ElasticMapsServer"),
    ];
    OWNER_LABELS
        .iter()
        .find_map(|(label, kind)| labels.get(*label).map(|name| (*kind, name.clone())))
}

#[derive(Debug, Default, PartialEq)]
pub struct Heap {
    pub min_bytes: Option<f64>,
    pub max_bytes: Option<f64>,
}

/// `-Xms`/`-Xmx` from `ES_JAVA_OPTS`. The last occurrence wins, matching the
/// JVM. Returns `None` when neither flag is set.
pub fn heap(java_opts: &str) -> Option<Heap> {
    let mut heap = Heap::default();
    for option in java_opts.split_whitespace() {
        if let Some(size) = option.strip_prefix("-Xms") {
            heap.min_bytes = jvm_size(size).or(heap.min_bytes);
        } else if let Some(size) = option.strip_prefix("-Xmx") {
            heap.max_bytes = jvm_size(size).or(heap.max_bytes);
        }
    }
    (heap != Heap::default()).then_some(heap)
}

fn jvm_size(size: &str) -> Option<f64> {
    let (number, power) = match size.chars().last()? {
        'k' | 'K' => (&size[..size.len() - 1], 1),
        'm' | 'M' => (&size[..size.len() - 1], 2),
        'g' | 'G' => (&size[..size.len() - 1], 3),
        't' | 'T' => (&size[..size.len() - 1], 4),
        _ => (size, 0),
    };
    let number = number.parse::<u64>().ok()? as f64;
    Some(number * 1024f64.powi(power))
}

/// The tag of a container image reference, ignoring registry ports and
/// digests: `registry:5000/eck/eck-operator:3.1.0@sha256:…` → `3.1.0`.
pub fn image_tag(image: &str) -> Option<&str> {
    let name = image.rsplit('/').next()?;
    let name = name.split('@').next()?;
    name.split_once(':').map(|(_, tag)| tag).filter(|tag| !tag.is_empty())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn labels(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn only_elastic_and_topology_labels_are_indexed() {
        let input = labels(&[
            ("elasticsearch.k8s.elastic.co/cluster-name", "quickstart"),
            ("common.k8s.elastic.co/type", "elasticsearch"),
            ("control-plane", "elastic-operator"),
            ("statefulset.kubernetes.io/pod-name", "quickstart-es-data-0"),
            ("topology.kubernetes.io/zone", "zone-a"),
            ("node-role.kubernetes.io/worker", ""),
            ("team", "payments"),
            ("app.kubernetes.io/name", "custom"),
            ("evil.k8s.elastic.co.example.com/x", "spoof"),
            ("k8s.elastic.co", "no-prefix"),
        ]);
        let kept = indexed_labels(&input);
        assert_eq!(kept.len(), 6);
        assert!(!kept.contains_key("team"));
        assert!(!kept.contains_key("app.kubernetes.io/name"));
        assert!(!kept.contains_key("evil.k8s.elastic.co.example.com/x"));
        assert!(!kept.contains_key("k8s.elastic.co"));
    }

    #[test]
    fn roles_and_owner_come_from_labels() {
        let input = labels(&[
            ("elasticsearch.k8s.elastic.co/cluster-name", "quickstart"),
            ("elasticsearch.k8s.elastic.co/node-data", "true"),
            ("elasticsearch.k8s.elastic.co/node-master", "false"),
            ("elasticsearch.k8s.elastic.co/node-ingest", "true"),
            ("node-role.kubernetes.io/control-plane", ""),
        ]);
        assert_eq!(elasticsearch_roles(&input), vec!["data", "ingest"]);
        assert_eq!(node_roles(&input), vec!["control-plane"]);
        assert_eq!(
            owning_resource(&input),
            Some(("Elasticsearch", "quickstart".to_string()))
        );
    }

    #[test]
    fn heap_is_parsed_only_when_set() {
        assert_eq!(
            heap("-Xms1g -Xmx2g -XX:+UseG1GC"),
            Some(Heap {
                min_bytes: Some(1_073_741_824.0),
                max_bytes: Some(2_147_483_648.0),
            })
        );
        assert_eq!(
            heap("-Xmx512m -Xmx768m"),
            Some(Heap {
                min_bytes: None,
                max_bytes: Some(805_306_368.0),
            })
        );
        assert_eq!(heap("-XX:+UseG1GC"), None);
        assert_eq!(heap("-Xmxlots"), None);
    }

    #[test]
    fn image_tags_ignore_registry_ports_and_digests() {
        assert_eq!(image_tag("docker.elastic.co/eck/eck-operator:3.1.0"), Some("3.1.0"));
        assert_eq!(
            image_tag("registry:5000/eck/eck-operator:3.1.0@sha256:abc"),
            Some("3.1.0")
        );
        assert_eq!(image_tag("registry:5000/eck/eck-operator"), None);
        assert_eq!(image_tag("eck-operator@sha256:abc"), None);
    }
}
