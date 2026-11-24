// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::data::{Node, Nodes};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};

/// The addresses a node can be found at, for correlating it with the host
/// that produced a bundle's system-call output.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct NodeEndpoint {
    pub id: String,
    pub name: String,
    pub host: Option<String>,
    pub ip: Option<String>,
    pub transport: Option<SocketAddr>,
    pub http_publish: Option<SocketAddr>,
    /// The hostname part of a `hostname/ip:port` HTTP publish address.
    pub http_publish_host: Option<String>,
    pub k8s_node_name: Option<String>,
}

#[derive(Deserialize)]
struct HttpInfo {
    publish_address: Option<String>,
}

#[derive(Deserialize)]
struct Attributes {
    k8s_node_name: Option<String>,
}

impl NodeEndpoint {
    /// Endpoints of every node, ordered by node ID.
    pub fn from_nodes(nodes: &Nodes) -> Vec<NodeEndpoint> {
        let mut endpoints: Vec<NodeEndpoint> = nodes.nodes.iter().map(|(id, node)| Self::from_node(id, node)).collect();
        endpoints.sort_by(|a, b| a.id.cmp(&b.id));
        endpoints
    }

    fn from_node(id: &str, node: &Node) -> NodeEndpoint {
        let publish = node
            .http
            .as_ref()
            .and_then(|http| serde_json::from_str::<HttpInfo>(http.get()).ok())
            .and_then(|http| http.publish_address);
        let (http_publish_host, http_publish) = match publish.as_deref().map(split_publish_address) {
            Some((host, address)) => (host, address),
            None => (None, None),
        };
        let k8s_node_name = node
            .attributes
            .as_ref()
            .and_then(|attributes| serde_json::from_str::<Attributes>(attributes.get()).ok())
            .and_then(|attributes| attributes.k8s_node_name);
        NodeEndpoint {
            id: id.to_string(),
            name: node.name.clone(),
            host: node.host.clone(),
            ip: node.ip.clone(),
            transport: node.transport_address.as_deref().and_then(parse_socket_addr),
            http_publish,
            http_publish_host,
            k8s_node_name,
        }
    }
}

/// Split `hostname/ip:port` or `ip:port` into its hostname and socket address.
pub fn split_publish_address(address: &str) -> (Option<String>, Option<SocketAddr>) {
    match address.split_once('/') {
        Some((host, socket)) => (
            Some(host.to_string()).filter(|host| !host.is_empty()),
            parse_socket_addr(socket),
        ),
        None => (None, parse_socket_addr(address)),
    }
}

/// Parse `ip:port`, `[v6]:port`, or an unbracketed `v6:port`, dropping any
/// `%zone` and mapping IPv4-mapped IPv6 addresses to IPv4.
pub fn parse_socket_addr(address: &str) -> Option<SocketAddr> {
    let (host, port) = address.rsplit_once(':')?;
    let port: u16 = port.parse().ok()?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let host = host.split_once('%').map_or(host, |(host, _)| host);
    let ip: IpAddr = host.parse().ok()?;
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
    };
    Some(SocketAddr::new(ip, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn socket(value: &str) -> SocketAddr {
        value.parse().unwrap()
    }

    #[test]
    fn publish_addresses_keep_hostname_and_socket() {
        assert_eq!(
            split_publish_address("es-01.example.com/10.0.0.10:9200"),
            (Some("es-01.example.com".to_string()), Some(socket("10.0.0.10:9200")))
        );
        assert_eq!(
            split_publish_address("10.0.0.10:9200"),
            (None, Some(socket("10.0.0.10:9200")))
        );
        assert_eq!(
            split_publish_address("es-01/[2001:db8::10]:9200"),
            (Some("es-01".to_string()), Some(socket("[2001:db8::10]:9200")))
        );
    }

    #[test]
    fn socket_addresses_normalize_ipv6_forms() {
        assert_eq!(
            parse_socket_addr("[2001:db8::10]:9300"),
            Some(socket("[2001:db8::10]:9300"))
        );
        assert_eq!(
            parse_socket_addr("2001:db8::10:9300"),
            Some(socket("[2001:db8::10]:9300"))
        );
        assert_eq!(
            parse_socket_addr("[::ffff:10.0.0.10]:9300"),
            Some(socket("10.0.0.10:9300"))
        );
        assert_eq!(parse_socket_addr("[fe80::1%eth0]:9300"), Some(socket("[fe80::1]:9300")));
        assert_eq!(parse_socket_addr("*:9300"), None);
        assert_eq!(parse_socket_addr("/run/socket"), None);
    }

    #[test]
    fn endpoints_come_from_the_parsed_node_list() {
        let node = |name: &str, transport: &str, publish: &str, k8s: Option<&str>| {
            json!({
                "name": name,
                "host": "10.0.0.10",
                "ip": "10.0.0.10",
                "transport_address": transport,
                "http": {"publish_address": publish},
                "attributes": k8s.map_or(json!({}), |k8s| json!({"k8s_node_name": k8s})),
                "build_flavor": "default",
                "build_hash": "test",
                "build_type": "tar",
                "jvm": {},
                "os": {"refresh_interval_in_millis": 1000, "available_processors": 4, "allocated_processors": 4},
                "process": {},
                "roles": ["data"],
                "thread_pool": {}
            })
        };
        let nodes: Nodes = serde_json::from_value(json!({
            "_nodes": {},
            "nodes": {
                "b": node("instance-1", "10.0.0.10:19301", "10.0.0.10:18001", None),
                "a": node("instance-0", "10.0.0.10:19300", "es-0.example.com/10.0.0.10:18000", Some("worker-3")),
            }
        }))
        .unwrap();

        let endpoints = NodeEndpoint::from_nodes(&nodes);
        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].id, "a");
        assert_eq!(endpoints[0].transport, Some(socket("10.0.0.10:19300")));
        assert_eq!(endpoints[0].http_publish, Some(socket("10.0.0.10:18000")));
        assert_eq!(endpoints[0].http_publish_host.as_deref(), Some("es-0.example.com"));
        assert_eq!(endpoints[0].k8s_node_name.as_deref(), Some("worker-3"));
        assert_eq!(endpoints[1].http_publish_host, None);
        assert_eq!(endpoints[1].k8s_node_name, None);
    }
}
