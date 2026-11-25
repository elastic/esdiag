// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Host identity and correlation of a SAR host with the Elasticsearch nodes
//! that run on it: none, one, or several (ECE allocators, Kubernetes nodes).

use super::header::SarHeader;
use crate::processor::elasticsearch::nodes::{NodeEndpoint, parse_socket_addr};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr};

/// Which listening-socket listing a file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketListing {
    /// `ss -an` style: `Netid State Recv-Q Send-Q Local Peer`, or without `Netid`.
    Ss,
    /// `netstat -an` style: `Proto Recv-Q Send-Q Local Foreign State`.
    Netstat,
}

/// Concrete, non-loopback TCP listening sockets from one `ss` or `netstat` line.
pub fn listening_socket(line: &str, listing: SocketListing) -> Option<SocketAddr> {
    let mut tokens = line.split_whitespace();
    let local = match listing {
        SocketListing::Ss => match tokens.next()? {
            "LISTEN" => tokens.nth(2)?,
            netid if netid.starts_with("tcp") && tokens.next()? == "LISTEN" => tokens.nth(2)?,
            _ => return None,
        },
        SocketListing::Netstat => {
            let proto = tokens.next()?;
            let local = tokens.nth(2)?;
            if !proto.starts_with("tcp") || tokens.nth(1)? != "LISTEN" {
                return None;
            }
            local
        }
    };
    parse_socket_addr(local).filter(|socket| is_concrete(socket.ip()))
}

fn is_concrete(ip: IpAddr) -> bool {
    !ip.is_unspecified() && !ip.is_loopback()
}

/// How the SAR host was tied to its nodes, stored in `node.match`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMethod {
    ListenSocket,
    K8sNodeName,
    Hostname,
    ManifestHost,
    None,
}

impl MatchMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            MatchMethod::ListenSocket => "listen_socket",
            MatchMethod::K8sNodeName => "k8s_node_name",
            MatchMethod::Hostname => "hostname",
            MatchMethod::ManifestHost => "manifest_host",
            MatchMethod::None => "none",
        }
    }
}

/// The nodes correlated with the host, and how.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeCorrelation {
    pub method: MatchMethod,
    pub nodes: Vec<NodeEndpoint>,
    /// The Kubernetes node name, when matched by `k8s_node_name`.
    pub kubernetes_node: Option<String>,
}

/// Correlation evidence, any of which may be absent.
pub struct CorrelationInput<'a> {
    pub hostname: &'a str,
    pub nodes: &'a [NodeEndpoint],
    pub listening: &'a [SocketAddr],
    pub manifest_host: Option<&'a str>,
}

fn short_name(name: &str) -> &str {
    name.split('.').next().unwrap_or(name)
}

/// The names a SAR hostname can match.
fn names(node: &NodeEndpoint) -> impl Iterator<Item = &str> {
    [
        node.host.as_deref(),
        Some(node.name.as_str()),
        node.http_publish_host.as_deref(),
    ]
    .into_iter()
    .flatten()
}

/// The machine a node reports running on.
fn host_value(node: &NodeEndpoint) -> &str {
    node.host
        .as_deref()
        .or(node.http_publish_host.as_deref())
        .unwrap_or(&node.name)
}

fn is_ip(value: &str) -> bool {
    value.trim_matches(['[', ']']).parse::<IpAddr>().is_ok()
}

/// Whether a manifest target host can identify a machine.
pub fn usable_manifest_host(host: &str) -> bool {
    let host = host.trim();
    !host.is_empty()
        && !host.eq_ignore_ascii_case("localhost")
        && !host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback() || ip.is_unspecified())
}

impl NodeCorrelation {
    /// Apply the methods in order and keep the first that matches a node.
    pub fn correlate(input: &CorrelationInput) -> NodeCorrelation {
        let found = |method, nodes: Vec<&NodeEndpoint>| NodeCorrelation {
            method,
            nodes: nodes.into_iter().cloned().collect(),
            kubernetes_node: None,
        };

        let by_socket: Vec<&NodeEndpoint> = input
            .nodes
            .iter()
            .filter(|node| {
                [node.transport, node.http_publish]
                    .into_iter()
                    .flatten()
                    .any(|address| input.listening.contains(&address))
            })
            .collect();
        if !by_socket.is_empty() {
            return found(MatchMethod::ListenSocket, by_socket);
        }

        let hostname = input.hostname;
        let by_k8s_name = |matches: &dyn Fn(&str) -> bool| -> Vec<&NodeEndpoint> {
            input
                .nodes
                .iter()
                .filter(|node| node.k8s_node_name.as_deref().is_some_and(matches))
                .collect()
        };
        let mut by_k8s = by_k8s_name(&|k8s| k8s.eq_ignore_ascii_case(hostname));
        if by_k8s.is_empty() {
            let short = short_name(hostname);
            by_k8s = by_k8s_name(&|k8s| short_name(k8s).eq_ignore_ascii_case(short));
            fn k8s_name(node: &NodeEndpoint) -> &str {
                node.k8s_node_name.as_deref().unwrap_or_default()
            }
            if let Some(first) = by_k8s.first()
                && !by_k8s
                    .iter()
                    .all(|node| k8s_name(node).eq_ignore_ascii_case(k8s_name(first)))
            {
                by_k8s.clear();
            }
        }
        if let Some(first) = by_k8s.first() {
            let kubernetes_node = first.k8s_node_name.clone();
            return NodeCorrelation {
                kubernetes_node,
                ..found(MatchMethod::K8sNodeName, by_k8s)
            };
        }

        let by_full: Vec<&NodeEndpoint> = input
            .nodes
            .iter()
            .filter(|node| names(node).any(|name| name.eq_ignore_ascii_case(hostname)))
            .collect();
        if !by_full.is_empty() {
            return found(MatchMethod::Hostname, by_full);
        }
        if !is_ip(hostname) {
            let short = short_name(hostname);
            let by_short: Vec<&NodeEndpoint> = input
                .nodes
                .iter()
                .filter(|node| names(node).any(|name| !is_ip(name) && short_name(name).eq_ignore_ascii_case(short)))
                .collect();
            if let Some(first) = by_short.first()
                && by_short
                    .iter()
                    .all(|node| host_value(node).eq_ignore_ascii_case(host_value(first)))
            {
                return found(MatchMethod::Hostname, by_short);
            }
        }

        if let Some(target) = input.manifest_host.filter(|host| usable_manifest_host(host)) {
            let by_manifest: Vec<&NodeEndpoint> = input
                .nodes
                .iter()
                .filter(|node| {
                    [
                        node.host.as_deref(),
                        node.ip.as_deref(),
                        node.http_publish_host.as_deref(),
                    ]
                    .into_iter()
                    .flatten()
                    .any(|value| value.eq_ignore_ascii_case(target))
                })
                .collect();
            if !by_manifest.is_empty() {
                return found(MatchMethod::ManifestHost, by_manifest);
            }
        }

        found(MatchMethod::None, Vec::new())
    }
}

/// The `host.*`, `node.*`, and `kubernetes.node.*` fields every
/// SAR document carries.
pub fn host_envelope(
    header: &SarHeader,
    listening: &[SocketAddr],
    correlation: &NodeCorrelation,
) -> Map<String, Value> {
    let mut host = Map::new();
    host.insert("name".into(), json!(header.hostname));
    host.insert("os".into(), json!({ "kernel": header.kernel }));
    if let Some(architecture) = &header.architecture {
        host.insert("architecture".into(), json!(architecture));
    }
    if let Some(count) = header.cpu_count {
        host.insert("cpu".into(), json!({ "count": count }));
    }
    let ips: BTreeSet<IpAddr> = listening.iter().map(SocketAddr::ip).collect();
    if !ips.is_empty() {
        host.insert(
            "ip".into(),
            json!(ips.iter().map(IpAddr::to_string).collect::<Vec<_>>()),
        );
    }

    let mut node = Map::new();
    if !correlation.nodes.is_empty() {
        node.insert(
            "id".into(),
            json!(correlation.nodes.iter().map(|node| &node.id).collect::<Vec<_>>()),
        );
        node.insert(
            "name".into(),
            json!(correlation.nodes.iter().map(|node| &node.name).collect::<Vec<_>>()),
        );
    }
    node.insert("match".into(), json!(correlation.method.as_str()));

    let mut envelope = Map::new();
    envelope.insert("host".into(), Value::Object(host));
    envelope.insert("node".into(), Value::Object(node));
    if let Some(kubernetes_node) = &correlation.kubernetes_node {
        envelope.insert("kubernetes".into(), json!({ "node": { "name": kubernetes_node } }));
    }
    envelope
}

/// The collection target from a manifest's `host='…'` input flag.
pub fn manifest_target_host(flags: &str) -> Option<String> {
    let start = flags.find("host='")? + "host='".len();
    let end = flags[start..].find('\'')? + start;
    let host = flags[start..end].trim();
    usable_manifest_host(host).then(|| host.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(id: &str, host: &str, transport: &str) -> NodeEndpoint {
        NodeEndpoint {
            id: id.to_string(),
            name: format!("instance-{id}"),
            host: Some(host.to_string()),
            ip: Some(host.to_string()),
            transport: transport.parse().ok(),
            ..NodeEndpoint::default()
        }
    }

    fn socket(value: &str) -> SocketAddr {
        value.parse().unwrap()
    }

    fn correlate(
        hostname: &str,
        nodes: &[NodeEndpoint],
        listening: &[SocketAddr],
        manifest: Option<&str>,
    ) -> NodeCorrelation {
        NodeCorrelation::correlate(&CorrelationInput {
            hostname,
            nodes,
            listening,
            manifest_host: manifest,
        })
    }

    fn ids(correlation: &NodeCorrelation) -> Vec<&str> {
        correlation.nodes.iter().map(|node| node.id.as_str()).collect()
    }

    #[test]
    fn parses_ss_and_netstat_listeners() {
        let ss = [
            "Netid State     Recv-Q Send-Q   Local Address:Port   Peer Address:Port Process",
            "tcp   LISTEN    0      128      10.0.0.10:9200      0.0.0.0:*",
            "tcp   LISTEN    0      128      [2001:db8::10]:9300  [::]:*",
            "tcp   LISTEN    0      128      0.0.0.0:22           0.0.0.0:*",
            "tcp   LISTEN    0      128      [::]:9200            [::]:*",
            "tcp   LISTEN    0      128      127.0.0.1:631        0.0.0.0:*",
            "tcp   ESTAB     0      0        10.0.0.10:9300      10.0.9.4:50122",
            "u_str LISTEN    0      128      /run/rpcbind.sock 21528  * 0",
            "LISTEN 0 4096 10.0.0.11:9300 0.0.0.0:*",
        ];
        let found: Vec<SocketAddr> = ss
            .iter()
            .filter_map(|line| listening_socket(line, SocketListing::Ss))
            .collect();
        assert_eq!(
            found,
            [
                socket("10.0.0.10:9200"),
                socket("[2001:db8::10]:9300"),
                socket("10.0.0.11:9300")
            ]
        );

        let netstat = [
            "Proto Recv-Q Send-Q Local Address           Foreign Address         State",
            "tcp        0      0 0.0.0.0:111             0.0.0.0:*               LISTEN",
            "tcp        0      0 10.0.0.10:9200         0.0.0.0:*               LISTEN",
            "tcp6       0      0 2001:db8::10:9300       :::*                    LISTEN",
            "tcp6       0      0 :::9200                 :::*                    LISTEN",
            "udp        0      0 10.0.0.10:123          0.0.0.0:*",
        ];
        let found: Vec<SocketAddr> = netstat
            .iter()
            .filter_map(|line| listening_socket(line, SocketListing::Netstat))
            .collect();
        assert_eq!(found, [socket("10.0.0.10:9200"), socket("[2001:db8::10]:9300")]);
    }

    #[test]
    fn ece_allocator_matches_every_local_node_and_excludes_others() {
        let nodes = [
            endpoint("a", "10.0.0.5", "10.0.0.5:18001"),
            endpoint("b", "10.0.0.5", "10.0.0.5:18002"),
            endpoint("c", "10.0.0.6", "10.0.0.6:18001"),
        ];
        let listening = [socket("10.0.0.5:18001"), socket("10.0.0.5:18002")];
        let correlation = correlate("allocator-1.example.com", &nodes, &listening, None);
        assert_eq!(correlation.method, MatchMethod::ListenSocket);
        assert_eq!(ids(&correlation), ["a", "b"]);
    }

    #[test]
    fn wildcard_binds_fall_through_to_later_methods() {
        let nodes = [endpoint("a", "es-01.example.com", "10.0.0.5:9300")];
        let listening: Vec<SocketAddr> = ["0.0.0.0:9300", "[::]:9300"]
            .iter()
            .filter_map(|line| listening_socket(&format!("tcp LISTEN 0 128 {line} *:*"), SocketListing::Ss))
            .collect();
        assert!(listening.is_empty());
        let correlation = correlate("es-01.example.com", &nodes, &listening, None);
        assert_eq!(correlation.method, MatchMethod::Hostname);
    }

    #[test]
    fn kubernetes_node_attribute_matches_every_pod_on_the_node() {
        let mut nodes = [
            endpoint("a", "10.244.1.5", "10.244.1.5:9300"),
            endpoint("b", "10.244.1.6", "10.244.1.6:9300"),
            endpoint("c", "10.244.2.7", "10.244.2.7:9300"),
        ];
        nodes[0].k8s_node_name = Some("worker-3".to_string());
        nodes[1].k8s_node_name = Some("WORKER-3".to_string());
        nodes[2].k8s_node_name = Some("worker-4".to_string());
        let correlation = correlate("worker-3", &nodes, &[], None);
        assert_eq!(correlation.method, MatchMethod::K8sNodeName);
        assert_eq!(ids(&correlation), ["a", "b"]);
        assert_eq!(correlation.kubernetes_node.as_deref(), Some("worker-3"));
    }

    #[test]
    fn kubernetes_short_names_match_only_one_qualified_node() {
        let mut nodes = [
            endpoint("a", "10.244.1.5", "10.244.1.5:9300"),
            endpoint("b", "10.244.1.6", "10.244.1.6:9300"),
            endpoint("c", "10.244.2.7", "10.244.2.7:9300"),
        ];
        nodes[0].k8s_node_name = Some("worker-3.dc1.example.com".to_string());
        nodes[1].k8s_node_name = Some("worker-3.dc1.example.com".to_string());
        nodes[2].k8s_node_name = Some("worker-3.dc2.example.com".to_string());

        let exact = correlate("WORKER-3.dc1.example.com", &nodes, &[], None);
        assert_eq!(exact.method, MatchMethod::K8sNodeName);
        assert_eq!(ids(&exact), ["a", "b"], "exact names take precedence over short names");

        let spanning = correlate("worker-3", &nodes, &[], None);
        assert_ne!(spanning.method, MatchMethod::K8sNodeName, "short name spans two nodes");
        assert!(spanning.kubernetes_node.is_none());

        let one_node = correlate("worker-3", &nodes[..2], &[], None);
        assert_eq!(one_node.method, MatchMethod::K8sNodeName);
        assert_eq!(ids(&one_node), ["a", "b"]);
    }

    #[test]
    fn hostname_matches_full_names_and_unambiguous_short_names() {
        let nodes = [endpoint("a", "es-01.dc1.example.com", "10.0.0.5:9300")];
        assert_eq!(
            correlate("ES-01.dc1.example.com", &nodes, &[], None).method,
            MatchMethod::Hostname
        );
        assert_eq!(correlate("es-01", &nodes, &[], None).method, MatchMethod::Hostname);
    }

    #[test]
    fn ambiguous_short_hostnames_do_not_match() {
        let nodes = [
            endpoint("a", "es-01.dc1.example.com", "10.0.0.5:9300"),
            endpoint("b", "es-01.dc2.example.com", "10.0.0.6:9300"),
        ];
        let correlation = correlate("es-01", &nodes, &[], None);
        assert_eq!(correlation.method, MatchMethod::None);
        assert!(correlation.nodes.is_empty());

        let named = |id: &str, host: &str| NodeEndpoint {
            name: "es-01".to_string(),
            ..endpoint(id, host, &format!("{host}:9300"))
        };
        let different_hosts = [named("a", "10.0.0.5"), named("b", "10.0.0.6")];
        let correlation = correlate("es-01.example.com", &different_hosts, &[], None);
        assert_eq!(correlation.method, MatchMethod::None, "same name, different machines");

        let same_host = [named("a", "10.0.0.5"), named("b", "10.0.0.5")];
        let correlation = correlate("es-01.example.com", &same_host, &[], None);
        assert_eq!(correlation.method, MatchMethod::Hostname);
        assert_eq!(ids(&correlation), ["a", "b"]);
    }

    #[test]
    fn manifest_host_matches_but_loopback_never_does() {
        let nodes = [endpoint("a", "10.0.0.5", "10.0.0.5:9300")];
        assert_eq!(
            correlate("unrelated", &nodes, &[], Some("10.0.0.5")).method,
            MatchMethod::ManifestHost
        );
        let mut local = endpoint("b", "localhost", "127.0.0.1:9300");
        local.ip = Some("127.0.0.1".to_string());
        for target in ["localhost", "127.0.0.1"] {
            assert_eq!(
                correlate("unrelated", std::slice::from_ref(&local), &[], Some(target)).method,
                MatchMethod::None
            );
        }
    }

    #[test]
    fn manifest_target_host_comes_from_the_input_flags() {
        assert_eq!(
            manifest_target_host(
                "BaseInputs{outputDir='/tmp'} ElasticRestClientInputs{host='es-01.example.com', port=9200}"
            ),
            Some("es-01.example.com".to_string())
        );
        assert_eq!(
            manifest_target_host("ElasticRestClientInputs{host='localhost', port=9200}"),
            None
        );
        assert_eq!(manifest_target_host("ElasticRestClientInputs{host='127.0.0.1'}"), None);
        assert_eq!(manifest_target_host("BaseInputs{outputDir='/tmp'}"), None);
    }

    #[test]
    fn envelope_lists_nodes_and_listening_ips() {
        let header =
            SarHeader::parse("Linux 6.1.0 (es-01.example.com) \t08/05/2026 \t_x86_64_\t(4 CPU)", None).unwrap();
        let nodes = [
            endpoint("a", "10.0.0.5", "10.0.0.5:18001"),
            endpoint("b", "10.0.0.5", "10.0.0.5:18002"),
        ];
        let listening = [socket("10.0.0.5:18001"), socket("10.0.0.5:18002")];
        let correlation = correlate("es-01.example.com", &nodes, &listening, None);
        let envelope = Value::Object(host_envelope(&header, &listening, &correlation));

        assert_eq!(envelope["host"]["name"], "es-01.example.com");
        assert_eq!(envelope["host"]["os"]["kernel"], "6.1.0");
        assert_eq!(envelope["host"]["architecture"], "x86_64");
        assert_eq!(envelope["host"]["cpu"]["count"], 4);
        assert_eq!(envelope["host"]["ip"], json!(["10.0.0.5"]));
        assert_eq!(
            envelope["node"],
            json!({
                "id": ["a", "b"],
                "name": ["instance-a", "instance-b"],
                "match": "listen_socket",
            })
        );
        assert!(envelope.get("kubernetes").is_none());

        let none = correlate("other", &[], &[], None);
        let envelope = Value::Object(host_envelope(&header, &[], &none));
        assert_eq!(envelope["node"], json!({"match": "none"}));
        assert!(envelope["host"].get("ip").is_none());
    }
}
