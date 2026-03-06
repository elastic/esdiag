// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::nodes::Nodes;
use dns_lookup::{lookup_addr, lookup_host};
use eyre::{Result, eyre};
use if_addrs::get_if_addrs;
use regex::Regex;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Clone, Default)]
pub struct LocalIdentity {
    pub hostnames: HashSet<String>,
    pub interface_names: HashSet<String>,
    pub interface_ips: HashSet<String>,
    pub interface_hostnames: HashSet<String>,
    pub canonical_hostnames: HashSet<String>,
}

impl LocalIdentity {
    pub fn all_signals(&self) -> HashSet<String> {
        self.hostnames
            .iter()
            .chain(self.interface_names.iter())
            .chain(self.interface_ips.iter())
            .chain(self.interface_hostnames.iter())
            .chain(self.canonical_hostnames.iter())
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct NodeMatch {
    pub node_id: String,
    pub host: Option<String>,
    pub ip: Option<String>,
}

#[derive(Debug, Clone)]
pub enum NodeMatchOutcome {
    Matched(NodeMatch),
    Multiple(NodeMatch, usize),
    NoMatch,
}

#[derive(Debug, Clone, Default)]
pub struct NodeRuntimeVars {
    pub pid: Option<String>,
    pub log_path: Option<String>,
    pub cluster_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SyscallInventory {
    #[serde(rename = "linuxOS")]
    pub linux_os: Option<SyscallGroups>,
    #[serde(rename = "macOS")]
    pub mac_os: Option<SyscallGroups>,
    #[serde(rename = "winOS")]
    pub win_os: Option<SyscallGroups>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SyscallGroups {
    pub sys: Option<BTreeMap<String, String>>,
    pub java: Option<BTreeMap<String, String>>,
    pub logs: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone)]
pub struct SyscallCommand {
    pub group: String,
    pub name: String,
    pub template: String,
}

#[derive(Debug, Clone)]
pub struct RenderedCommand {
    pub command: String,
    pub unresolved: Vec<String>,
}

pub fn gather_local_identity() -> LocalIdentity {
    let mut identity = LocalIdentity::default();

    if let Ok(host) = hostname::get()
        && let Some(host) = host.to_str()
    {
        let normalized = normalize(host);
        if !normalized.is_empty() {
            identity.hostnames.insert(normalized.clone());
            identity.canonical_hostnames.insert(normalized.clone());

            if let Ok(addrs) = lookup_host(&normalized) {
                for addr in addrs {
                    if let Ok(reverse) = lookup_addr(&addr) {
                        let rev = normalize(&reverse);
                        if !rev.is_empty() {
                            identity.interface_hostnames.insert(rev.clone());
                            identity.canonical_hostnames.insert(rev);
                        }
                    }
                }
            }
        }
    }

    if let Ok(ifaces) = get_if_addrs() {
        for iface in ifaces {
            let name = normalize(&iface.name);
            if !name.is_empty() {
                identity.interface_names.insert(name);
            }

            let ip = normalize(&iface.ip().to_string());
            if !ip.is_empty() {
                identity.interface_ips.insert(ip.clone());
            }

            if let Ok(reverse) = lookup_addr(&iface.ip()) {
                let reverse = normalize(&reverse);
                if !reverse.is_empty() {
                    identity.interface_hostnames.insert(reverse.clone());
                    identity.canonical_hostnames.insert(reverse);
                }
            }
        }
    }

    identity
}

pub fn match_node(nodes: &Nodes, identity: &LocalIdentity) -> NodeMatchOutcome {
    let signals = identity.all_signals();

    let mut ids: Vec<&String> = nodes.nodes.keys().collect();
    ids.sort();

    let mut matches = Vec::new();
    for node_id in ids {
        if let Some(node) = nodes.nodes.get(node_id) {
            let host_match = node
                .host
                .as_ref()
                .map(|h| signals.contains(&normalize(h)))
                .unwrap_or(false);
            let ip_match = node
                .ip
                .as_ref()
                .map(|ip| signals.contains(&normalize(ip)))
                .unwrap_or(false);

            if host_match || ip_match {
                matches.push(NodeMatch {
                    node_id: node_id.clone(),
                    host: node.host.clone(),
                    ip: node.ip.clone(),
                });
            }
        }
    }

    match matches.len() {
        0 => NodeMatchOutcome::NoMatch,
        1 => NodeMatchOutcome::Matched(matches.remove(0)),
        n => NodeMatchOutcome::Multiple(matches.remove(0), n),
    }
}

pub fn extract_node_runtime_vars(nodes: &Nodes, node_id: &str) -> NodeRuntimeVars {
    let Some(node) = nodes.nodes.get(node_id) else {
        return NodeRuntimeVars::default();
    };

    node.runtime_vars()
}

pub fn load_inventory() -> Result<&'static SyscallInventory> {
    static INVENTORY: OnceLock<SyscallInventory> = OnceLock::new();
    if let Some(inventory) = INVENTORY.get() {
        return Ok(inventory);
    }

    let parsed: SyscallInventory =
        serde_yaml::from_str(include_str!("../../../assets/elasticsearch/syscalls.yml"))
            .map_err(|e| eyre!("failed to parse assets/elasticsearch/syscalls.yml: {}", e))?;

    let _ = INVENTORY.set(parsed);
    INVENTORY
        .get()
        .ok_or_else(|| eyre!("failed to initialize syscall inventory"))
}

pub fn select_commands_for_os(inventory: &SyscallInventory, os: &str) -> Option<Vec<SyscallCommand>> {
    let groups = match os {
        "linux" => inventory.linux_os.as_ref(),
        "macos" => inventory.mac_os.as_ref(),
        "windows" => inventory.win_os.as_ref(),
        _ => None,
    }?;

    let mut commands = Vec::new();

    if let Some(sys) = &groups.sys {
        for (name, template) in sys {
            commands.push(SyscallCommand {
                group: "sys".to_string(),
                name: name.to_string(),
                template: template.to_string(),
            });
        }
    }

    if let Some(java) = &groups.java {
        for (name, template) in java {
            commands.push(SyscallCommand {
                group: "java".to_string(),
                name: name.to_string(),
                template: template.to_string(),
            });
        }
    }

    if let Some(logs) = &groups.logs {
        for (name, template) in logs {
            commands.push(SyscallCommand {
                group: "logs".to_string(),
                name: name.to_string(),
                template: template.to_string(),
            });
        }
    }

    Some(commands)
}

pub fn render_command(template: &str, variables: &HashMap<String, String>) -> RenderedCommand {
    let placeholder_re = placeholder_regex();
    let mut unresolved = Vec::new();

    let command = placeholder_re
        .replace_all(template, |caps: &regex::Captures<'_>| {
            let name = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            match variables.get(name) {
                Some(value) if !value.trim().is_empty() => value.to_string(),
                _ => {
                    unresolved.push(name.to_string());
                    caps.get(0)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default()
                }
            }
        })
        .to_string();

    unresolved.sort();
    unresolved.dedup();

    RenderedCommand {
        command,
        unresolved,
    }
}

pub fn infer_java_home_from_process_listing(pid: &str, process_listing: &str) -> Option<String> {
    for line in process_listing.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let cols: Vec<&str> = trimmed.split_whitespace().collect();
        if cols.len() < 8 {
            continue;
        }

        if cols.get(1).copied() != Some(pid) {
            continue;
        }

        let cmd = cols[7..].join(" ");
        let executable = cmd.split_whitespace().next().unwrap_or_default();
        if executable.is_empty() {
            continue;
        }

        let executable = executable.trim_matches('"');
        let path = Path::new(executable);
        let parent = path.parent()?;
        let java_home = if parent.ends_with("bin") {
            parent.parent().unwrap_or(parent)
        } else {
            parent
        };

        let value = java_home.to_string_lossy().to_string();
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

pub fn process_listing_command_for_os(os: &str, pid: &str) -> String {
    match os {
        "windows" => format!("wmic process where processId={} GET CommandLine", pid),
        _ => "ps -ef".to_string(),
    }
}

pub fn sanitize_for_filename(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn placeholder_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{\{([A-Z0-9_]+)\}\}").expect("valid placeholder regex"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processor::elasticsearch::nodes::Nodes;

        fn sample_nodes() -> Nodes {
                let nodes_json = r#"
                {
                    "_nodes": {},
                    "nodes": {
                        "a-node": {
                            "attributes": null,
                            "build_flavor": "default",
                            "build_hash": "x",
                            "build_type": "docker",
                            "host": "10.0.0.1",
                            "ip": "10.0.0.1",
                            "jvm": {},
                            "name": "node-a",
                            "os": {
                                "refresh_interval_in_millis": 1000,
                                "name": "Linux",
                                "pretty_name": "Linux",
                                "arch": "amd64",
                                "version": "1",
                                "available_processors": 2,
                                "allocated_processors": 2
                            },
                            "plugins": null,
                            "process": {"id": 41},
                            "roles": ["master"],
                            "settings": {
                                "path": {"logs": "/var/log/es-a"},
                                "cluster": {"name": "cluster-a"}
                            },
                            "thread_pool": {}
                        },
                        "b-node": {
                            "attributes": null,
                            "build_flavor": "default",
                            "build_hash": "x",
                            "build_type": "docker",
                            "host": "10.0.0.2",
                            "ip": "10.0.0.2",
                            "jvm": {},
                            "name": "node-b",
                            "os": {
                                "refresh_interval_in_millis": 1000,
                                "name": "Linux",
                                "pretty_name": "Linux",
                                "arch": "amd64",
                                "version": "1",
                                "available_processors": 2,
                                "allocated_processors": 2
                            },
                            "plugins": null,
                            "process": {"id": 42},
                            "roles": ["master"],
                            "settings": {"path": {"logs": "/var/log/es-b"}},
                            "thread_pool": {}
                        }
                    }
                }
                "#;

                serde_json::from_str(nodes_json).expect("valid nodes JSON")
        }

    #[test]
    fn render_command_reports_unresolved_placeholders() {
        let mut vars = HashMap::new();
        vars.insert("PID".to_string(), "123".to_string());

        let rendered = render_command("jstack {{PID}} {{JAVA_HOME}}", &vars);
        assert_eq!(rendered.command, "jstack 123 {{JAVA_HOME}}");
        assert_eq!(rendered.unresolved, vec!["JAVA_HOME"]);
    }

    #[test]
    fn infer_java_home_parses_ps_output() {
        let ps = "root 464 1 0 00:00 ? 00:00:01 /usr/share/elasticsearch/jdk/bin/java -Xmx2g";
        let java_home = infer_java_home_from_process_listing("464", ps);
        assert_eq!(java_home.as_deref(), Some("/usr/share/elasticsearch/jdk"));
    }

        #[test]
        fn gather_local_identity_collects_at_least_one_signal() {
                let identity = gather_local_identity();
                let signals = identity.all_signals();
                assert!(
                        !signals.is_empty(),
                        "expected local identity gathering to capture at least one signal"
                );
        }

    #[test]
    fn node_match_uses_first_deterministic_match() {
                let nodes = sample_nodes();
        let mut identity = LocalIdentity::default();
        identity.interface_ips.insert("10.0.0.1".to_string());
        identity.interface_ips.insert("10.0.0.2".to_string());

        let outcome = match_node(&nodes, &identity);
        match outcome {
            NodeMatchOutcome::Multiple(selected, count) => {
                assert_eq!(count, 2);
                assert_eq!(selected.node_id, "a-node");
            }
            _ => panic!("expected multiple node match outcome"),
        }
    }

    #[test]
    fn node_match_reports_no_match_when_signals_do_not_match() {
        let nodes = sample_nodes();
        let mut identity = LocalIdentity::default();
        identity.interface_ips.insert("192.168.99.99".to_string());

        let outcome = match_node(&nodes, &identity);
        assert!(matches!(outcome, NodeMatchOutcome::NoMatch));
    }

    #[test]
    fn extract_runtime_vars_reads_pid_logpath_and_cluster() {
        let nodes = sample_nodes();
        let vars = extract_node_runtime_vars(&nodes, "a-node");

        assert_eq!(vars.pid.as_deref(), Some("41"));
        assert_eq!(vars.log_path.as_deref(), Some("/var/log/es-a"));
        assert_eq!(vars.cluster_name.as_deref(), Some("cluster-a"));
    }
}
