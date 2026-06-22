// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use eyre::Result;
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Fields mapped as `ip` or keyword mirrors of node addresses in esdiag exports.
const PURE_IP_FIELDS: &[&str] = &["ip", "host", "publish_host", "bind_host"];
const IP_OR_PORT_FIELDS: &[&str] = &[
    "transport_address",
    "publish_address",
    "bound_address",
    "local_address",
    "remote_address",
    "x_forwarded_for",
];
const EXCLUDED_JSON_FILES: &[&str] = &["diagnostic_manifest.json", "version.json"];

pub struct TransformResult {
    pub content: String,
    pub transformed_fields: usize,
    pub supported: bool,
}

pub struct TempTransformResult {
    pub file: tempfile::NamedTempFile,
    pub transformed_fields: usize,
}

pub fn supports_json_normalization(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let filename = Path::new(&normalized)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    filename.ends_with(".json") && !EXCLUDED_JSON_FILES.contains(&filename)
}

pub fn normalize_supported_content(path: &str, input: String) -> Result<TransformResult> {
    if !supports_json_normalization(path) {
        return Ok(TransformResult {
            content: input,
            transformed_fields: 0,
            supported: false,
        });
    }

    let mut json: Value = serde_json::from_str(&input)?;
    let mut transformed = 0usize;
    normalize_value(&mut json, &mut Vec::new(), &mut transformed);

    Ok(TransformResult {
        content: serde_json::to_string(&json)?,
        transformed_fields: transformed,
        supported: true,
    })
}

pub fn normalize_supported_reader_to_temp<R: Read>(path: &str, reader: R) -> Result<TempTransformResult> {
    let mut file = tempfile::NamedTempFile::new()?;
    if !supports_json_normalization(path) {
        return Ok(TempTransformResult {
            file,
            transformed_fields: 0,
        });
    }

    let mut json: Value = serde_json::from_reader(reader)?;
    let mut transformed = 0usize;
    normalize_value(&mut json, &mut Vec::new(), &mut transformed);
    serde_json::to_writer(&mut file, &json)?;
    file.as_file_mut().seek(SeekFrom::Start(0))?;

    Ok(TempTransformResult {
        file,
        transformed_fields: transformed,
    })
}

fn normalize_value(value: &mut Value, path: &mut Vec<String>, transformed: &mut usize) {
    if matches_http_client_id(path)
        && let Value::String(raw) = value
        && let Some(client_id) = normalize_http_client_id(raw)
    {
        *value = Value::Number(client_id.into());
        *transformed += 1;
        return;
    }

    match value {
        Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                path.push(key.clone());
                normalize_value(child, path, transformed);
                path.pop();
            }
        }
        Value::Array(array) => {
            for child in array.iter_mut() {
                normalize_value(child, path, transformed);
            }
        }
        Value::String(raw) => {
            let Some(key) = path.last().map(String::as_str) else {
                return;
            };

            let normalized = if PURE_IP_FIELDS.contains(&key) {
                normalize_pure_ip(raw)
            } else if IP_OR_PORT_FIELDS.contains(&key) {
                normalize_ip_or_ip_port(raw)
            } else {
                None
            };

            if let Some(updated) = normalized
                && updated != *raw
            {
                *raw = updated;
                *transformed += 1;
            }
        }
        _ => {}
    }
}

fn normalize_pure_ip(value: &str) -> Option<String> {
    let (candidate_ip, port) = split_ip_port(value);
    if port.is_none() {
        return normalize_malformed_ipv4(candidate_ip);
    }

    normalize_malformed_ipv4(candidate_ip).or_else(|| parse_ipv4_octets(candidate_ip).map(|_| candidate_ip.to_string()))
}

fn normalize_ip_or_ip_port(value: &str) -> Option<String> {
    let (candidate_ip, port) = split_ip_port(value);
    let normalized_ip = normalize_malformed_ipv4(candidate_ip)?;
    match port {
        Some(port) => Some(format!("{normalized_ip}:{port}")),
        None => Some(normalized_ip),
    }
}

fn split_ip_port(value: &str) -> (&str, Option<&str>) {
    let Some((ip, port)) = value.rsplit_once(':') else {
        return (value, None);
    };

    if ip.contains(':') || !port.chars().all(|c| c.is_ascii_digit()) {
        return (value, None);
    }

    (ip, Some(port))
}

fn normalize_malformed_ipv4(value: &str) -> Option<String> {
    let octets = parse_ipv4_octets(value)?;

    if octets.iter().all(|octet| *octet <= 255) {
        return None;
    }

    Some(
        octets
            .iter()
            .map(|octet| (octet % 255).to_string())
            .collect::<Vec<String>>()
            .join("."),
    )
}

fn normalize_http_client_id(value: &str) -> Option<u64> {
    let octets = parse_ipv4_octets(value)?;
    if octets.iter().all(|octet| *octet <= 255) {
        return None;
    }

    let normalized = octets.map(|octet| octet % 255);
    let id = ((normalized[0] as u64) << 24)
        | ((normalized[1] as u64) << 16)
        | ((normalized[2] as u64) << 8)
        | normalized[3] as u64;
    Some(id)
}

fn matches_http_client_id(path: &[String]) -> bool {
    path.len() >= 3
        && path[path.len() - 3] == "http"
        && path[path.len() - 2] == "clients"
        && path[path.len() - 1] == "id"
}

fn parse_ipv4_octets(value: &str) -> Option<[u16; 4]> {
    let mut octets = [0u16; 4];
    let mut parts = value.split('.');
    for octet in &mut octets {
        let part = parts.next()?;
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        *octet = part.parse().ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(octets)
}

#[cfg(test)]
pub(super) mod synthetic_vectors {
    // Hand-authored scrub test vectors only. Do NOT copy values from customer diagnostics.

    /// Malformed IPv4-like string used in unit/integration tests (each octet > 255).
    pub const MALFORMED_IP: &str = "512.768.1024.1280";
    pub const MALFORMED_IP_WITH_PORT: &str = "512.768.1024.1280:19840";
    pub const NORMALIZED_IP: &str = "2.3.4.5";
    pub const NORMALIZED_IP_WITH_PORT: &str = "2.3.4.5:19840";

    pub const MALFORMED_IP_SECONDARY: &str = "513.769.1025.1281";
    pub const NORMALIZED_IP_SECONDARY: &str = "3.4.5.6";
    pub const MALFORMED_IP_SECONDARY_WITH_PORT: &str = "513.769.1025.1281:19033";
    pub const NORMALIZED_IP_SECONDARY_WITH_PORT: &str = "3.4.5.6:19033";

    pub const MALFORMED_HTTP_CLIENT_ID: &str = "516.772.1028.1284";
    pub const NORMALIZED_HTTP_CLIENT_ID: u64 = 101_124_105;

    /// RFC 5737 TEST-NET-1 address for valid pass-through cases.
    pub const VALID_IP: &str = "192.0.2.50";
    pub const VALID_IP_WITH_PORT: &str = "192.0.2.50:9300";

    /// 19-char lowercase hex node id/name for scrub humanization tests.
    pub const SYNTHETIC_HEX_NODE_ID: &str = "aaaabbbbccccddddee0";
}

#[cfg(test)]
mod tests {
    use super::synthetic_vectors as v;
    use super::*;

    #[test]
    fn normalizes_supported_ip_fields_only() {
        let input = format!(
            r#"{{
            "nodes": {{
                "a": {{
                    "ip": "{malformed}",
                    "host": "{malformed_port}",
                    "transport_address": "{malformed_port}",
                    "name": "{malformed}",
                    "transport": {{
                        "publish_address": "{malformed_secondary_port}",
                        "x_forwarded_for": "{malformed_secondary}"
                    }},
                    "http": {{
                        "clients": [
                            {{ "id": "{malformed_client_id}" }}
                        ]
                    }}
                }}
            }}
        }}"#,
            malformed = v::MALFORMED_IP,
            malformed_port = v::MALFORMED_IP_WITH_PORT,
            malformed_secondary = v::MALFORMED_IP_SECONDARY,
            malformed_secondary_port = v::MALFORMED_IP_SECONDARY_WITH_PORT,
            malformed_client_id = v::MALFORMED_HTTP_CLIENT_ID,
        );

        let result = normalize_supported_content("diag/nodes.json", input).expect("normalize");

        assert!(result.supported);
        assert_eq!(result.transformed_fields, 6);
        assert!(result.content.contains(&format!("\"ip\":\"{}\"", v::NORMALIZED_IP)));
        assert!(result.content.contains(&format!("\"host\":\"{}\"", v::NORMALIZED_IP)));
        assert!(
            result
                .content
                .contains(&format!("\"transport_address\":\"{}\"", v::NORMALIZED_IP_WITH_PORT))
        );
        assert!(result.content.contains(&format!(
            "\"publish_address\":\"{}\"",
            v::NORMALIZED_IP_SECONDARY_WITH_PORT
        )));
        assert!(
            result
                .content
                .contains(&format!("\"x_forwarded_for\":\"{}\"", v::NORMALIZED_IP_SECONDARY))
        );
        assert!(result.content.contains(&format!("\"name\":\"{}\"", v::MALFORMED_IP)));
        assert!(
            result
                .content
                .contains(&format!("\"id\":{}", v::NORMALIZED_HTTP_CLIENT_ID))
        );
    }

    #[test]
    fn skips_unsupported_files() {
        let input = format!("{{\"ip\":\"{}\"}}", v::MALFORMED_IP);
        let result = normalize_supported_content("diag/version.json", input.clone()).expect("normalize");
        assert!(!result.supported);
        assert_eq!(result.transformed_fields, 0);
        assert_eq!(result.content, input);
    }

    #[test]
    fn does_not_match_files_with_tasks_suffix_only() {
        assert!(supports_json_normalization("diag/cluster_pending_tasks.json"));
        assert!(supports_json_normalization("diag/tasks.json"));
    }

    #[test]
    fn normalizes_publish_host_and_bind_host() {
        let input = format!(
            r#"{{
            "nodes": {{
                "a": {{
                    "ip": "{malformed}",
                    "settings": {{
                        "network": {{
                            "publish_host": "{malformed}",
                            "bind_host": "{malformed}"
                        }}
                    }}
                }}
            }}
        }}"#,
            malformed = v::MALFORMED_IP,
        );

        let result = normalize_supported_content("diag/nodes.json", input).expect("normalize");

        assert!(result.supported);
        assert_eq!(result.transformed_fields, 3);
        assert!(
            result
                .content
                .contains(&format!("\"publish_host\":\"{}\"", v::NORMALIZED_IP))
        );
        assert!(
            result
                .content
                .contains(&format!("\"bind_host\":\"{}\"", v::NORMALIZED_IP))
        );
    }

    #[test]
    fn normalizes_ip_fields_in_other_diagnostic_json_files() {
        let input = format!(
            r#"{{"master_node":{{"ip":"{malformed}","host":"{malformed}"}}}}"#,
            malformed = v::MALFORMED_IP
        );
        let result = normalize_supported_content("diag/master.json", input).expect("normalize");
        assert!(result.supported);
        assert_eq!(result.transformed_fields, 2);
    }

    #[test]
    fn normalizes_tasks_json_malformed_addresses() {
        let input = format!(
            r#"{{
            "nodes": {{
                "node-a": {{
                    "tasks": {{
                        "1": {{
                            "action": "indices:data/write/bulk",
                            "description": "bulk",
                            "running_time_in_nanos": 100,
                            "start_time_in_millis": 1,
                            "type": "transport",
                            "headers": {{}}
                        }}
                    }},
                    "host": "{malformed}",
                    "ip": "{malformed_port}",
                    "transport_address": "{malformed_port}"
                }}
            }}
        }}"#,
            malformed = v::MALFORMED_IP,
            malformed_port = v::MALFORMED_IP_WITH_PORT,
        );

        let result = normalize_supported_content("diag/tasks.json", input).expect("normalize");

        assert!(result.supported);
        assert_eq!(result.transformed_fields, 3);
        assert!(result.content.contains(&format!("\"host\":\"{}\"", v::NORMALIZED_IP)));
        assert!(result.content.contains(&format!("\"ip\":\"{}\"", v::NORMALIZED_IP)));
        assert!(
            result
                .content
                .contains(&format!("\"transport_address\":\"{}\"", v::NORMALIZED_IP_WITH_PORT))
        );
    }

    #[test]
    fn leaves_valid_ipv4_unchanged() {
        let input = format!(
            r#"{{
            "nodes": {{
                "a": {{
                    "ip": "{valid}",
                    "host": "{valid}",
                    "transport_address": "{valid_port}",
                    "transport": {{
                        "publish_address": "{valid_port}"
                    }},
                    "http": {{
                        "clients": [
                            {{ "id": "{valid}" }}
                        ]
                    }}
                }}
            }}
        }}"#,
            valid = v::VALID_IP,
            valid_port = v::VALID_IP_WITH_PORT,
        );

        let result = normalize_supported_content("diag/nodes.json", input).expect("normalize");

        assert!(result.supported);
        assert_eq!(result.transformed_fields, 0);
        assert!(result.content.contains(&format!("\"ip\":\"{}\"", v::VALID_IP)));
        assert!(
            result
                .content
                .contains(&format!("\"transport_address\":\"{}\"", v::VALID_IP_WITH_PORT))
        );
        assert!(result.content.contains(&format!("\"id\":\"{}\"", v::VALID_IP)));
    }

    #[test]
    fn strips_valid_ports_from_pure_ip_fields() {
        let input = format!(
            r#"{{
            "nodes": {{
                "a": {{
                    "ip": "{valid_port}",
                    "host": "{valid_port}",
                    "publish_host": "{valid_port}",
                    "transport_address": "{valid_port}"
                }}
            }}
        }}"#,
            valid_port = v::VALID_IP_WITH_PORT,
        );

        let result = normalize_supported_content("diag/nodes.json", input).expect("normalize");

        assert!(result.supported);
        assert_eq!(result.transformed_fields, 3);
        assert!(result.content.contains(&format!("\"ip\":\"{}\"", v::VALID_IP)));
        assert!(result.content.contains(&format!("\"host\":\"{}\"", v::VALID_IP)));
        assert!(
            result
                .content
                .contains(&format!("\"publish_host\":\"{}\"", v::VALID_IP))
        );
        assert!(
            result
                .content
                .contains(&format!("\"transport_address\":\"{}\"", v::VALID_IP_WITH_PORT))
        );
    }
}
