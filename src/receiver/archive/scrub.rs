// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use eyre::Result;
use serde_json::Value;
use std::io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write};
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

/// Keep temporary normalized content alive for either eager or streaming deserialization.
pub(crate) fn with_normalized_json_reader<R: BufRead, T>(
    path: &str,
    mut reader: R,
    scrubbed: bool,
    deserialize: impl FnOnce(&mut dyn BufRead) -> Result<T>,
) -> Result<T> {
    if scrubbed && supports_json_normalization(path) {
        let mut transformed = normalize_supported_reader_to_temp(path, reader)?;
        tracing::debug!(
            "Unscrubbed {} address fields in {}",
            transformed.transformed_fields,
            path
        );
        deserialize(&mut BufReader::new(transformed.file.as_file_mut()))
    } else {
        if scrubbed {
            tracing::debug!("Scrubbed mode read {} (no normalization rules)", path);
        }
        deserialize(&mut reader)
    }
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
    normalize_value(&mut json, None, &mut transformed);

    Ok(TransformResult {
        content: serde_json::to_string(&json)?,
        transformed_fields: transformed,
        supported: true,
    })
}

pub fn normalize_supported_reader_to_temp<R: BufRead>(path: &str, mut reader: R) -> Result<TempTransformResult> {
    let mut file = tempfile::NamedTempFile::new()?;
    if !supports_json_normalization(path) {
        std::io::copy(&mut reader, &mut file)?;
        file.as_file_mut().seek(SeekFrom::Start(0))?;
        return Ok(TempTransformResult {
            file,
            transformed_fields: 0,
        });
    }

    let mut transformed = 0usize;
    {
        let mut writer = BufWriter::new(file.as_file_mut());
        normalize_json_stream(reader, &mut writer, &mut transformed)?;
        writer.flush()?;
    }
    file.as_file_mut().seek(SeekFrom::Start(0))?;

    Ok(TempTransformResult {
        file,
        transformed_fields: transformed,
    })
}

fn normalize_json_stream<R: BufRead, W: Write>(reader: R, writer: &mut W, transformed: &mut usize) -> Result<()> {
    let mut bytes = reader.bytes().peekable();
    let mut stack = Vec::<JsonContext>::new();

    while let Some(byte) = bytes.next() {
        let byte = byte?;
        match byte {
            b'"' => {
                let raw = read_json_string_body(&mut bytes)?;
                if is_object_key_context(&stack) {
                    let key = serde_json::from_str::<String>(&format!("\"{raw}\""))?;
                    write!(writer, "\"{raw}\"")?;
                    if let Some(JsonContext::Object { pending_key, .. }) = stack.last_mut() {
                        *pending_key = Some(key);
                    }
                } else {
                    let value = serde_json::from_str::<String>(&format!("\"{raw}\""))?;
                    match normalize_string_for_field(field_for_value(&stack), &value) {
                        Some(updated) if updated != value => {
                            serde_json::to_writer(&mut *writer, &updated)?;
                            *transformed += 1;
                        }
                        _ => write!(writer, "\"{raw}\"")?,
                    }
                    mark_value_written(&mut stack);
                }
            }
            b'{' => {
                writer.write_all(b"{")?;
                stack.push(JsonContext::Object {
                    pending_key: None,
                    expecting_key: true,
                });
            }
            b'[' => {
                let field = match stack.last_mut() {
                    Some(JsonContext::Object { pending_key, .. }) => pending_key.take(),
                    Some(JsonContext::Array { field }) => field.clone(),
                    None => None,
                };
                writer.write_all(b"[")?;
                stack.push(JsonContext::Array { field });
            }
            b'}' | b']' => {
                writer.write_all(&[byte])?;
                stack.pop();
                mark_value_written(&mut stack);
            }
            b',' => {
                writer.write_all(b",")?;
                if let Some(JsonContext::Object {
                    pending_key,
                    expecting_key,
                    ..
                }) = stack.last_mut()
                {
                    *pending_key = None;
                    *expecting_key = true;
                }
            }
            b':' => {
                writer.write_all(b":")?;
                if let Some(JsonContext::Object { expecting_key, .. }) = stack.last_mut() {
                    *expecting_key = false;
                }
            }
            byte if byte.is_ascii_whitespace() => writer.write_all(&[byte])?,
            byte => {
                writer.write_all(&[byte])?;
                copy_literal_tail(&mut bytes, writer)?;
                mark_value_written(&mut stack);
            }
        }
    }

    Ok(())
}

enum JsonContext {
    Object {
        pending_key: Option<String>,
        expecting_key: bool,
    },
    Array {
        field: Option<String>,
    },
}

fn read_json_string_body<I>(bytes: &mut std::iter::Peekable<I>) -> Result<String>
where
    I: Iterator<Item = std::io::Result<u8>>,
{
    let mut raw = Vec::new();
    let mut escaped = false;
    for byte in bytes.by_ref() {
        let byte = byte?;
        if escaped {
            raw.push(byte);
            escaped = false;
        } else if byte == b'\\' {
            raw.push(byte);
            escaped = true;
        } else if byte == b'"' {
            break;
        } else {
            raw.push(byte);
        }
    }
    Ok(String::from_utf8(raw)?)
}

fn is_object_key_context(stack: &[JsonContext]) -> bool {
    matches!(
        stack.last(),
        Some(JsonContext::Object {
            expecting_key: true,
            ..
        })
    )
}

fn mark_value_written(stack: &mut [JsonContext]) {
    if let Some(JsonContext::Object {
        pending_key,
        expecting_key,
        ..
    }) = stack.last_mut()
    {
        *pending_key = None;
        *expecting_key = false;
    }
}

fn field_for_value(stack: &[JsonContext]) -> Option<&str> {
    match stack.last()? {
        JsonContext::Object { pending_key, .. } => pending_key.as_deref(),
        JsonContext::Array { field } => field.as_deref(),
    }
}

fn normalize_string_for_field(field: Option<&str>, raw: &str) -> Option<String> {
    let key = field?;
    if PURE_IP_FIELDS.contains(&key) {
        normalize_pure_ip(raw)
    } else if IP_OR_PORT_FIELDS.contains(&key) {
        normalize_ip_or_ip_port(raw)
    } else {
        None
    }
}

fn copy_literal_tail<I, W>(bytes: &mut std::iter::Peekable<I>, writer: &mut W) -> Result<()>
where
    I: Iterator<Item = std::io::Result<u8>>,
    W: Write,
{
    while let Some(byte) = bytes.peek() {
        let byte = match byte {
            Ok(byte) => *byte,
            Err(_) => break,
        };
        if byte == b',' || byte == b'}' || byte == b']' || byte.is_ascii_whitespace() {
            break;
        }
        let byte = bytes.next().transpose()?.expect("peeked byte");
        writer.write_all(&[byte])?;
    }
    Ok(())
}

fn normalize_value(value: &mut Value, field: Option<&str>, transformed: &mut usize) {
    match value {
        Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                normalize_value(child, Some(key), transformed);
            }
        }
        Value::Array(array) => {
            for child in array.iter_mut() {
                normalize_value(child, field, transformed);
            }
        }
        Value::String(raw) => {
            if let Some(updated) = normalize_string_for_field(field, raw)
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
    let octets = parse_ipv4_octets(candidate_ip)?;
    normalize_malformed_ipv4(octets).or_else(|| port.map(|_| candidate_ip.to_string()))
}

fn normalize_ip_or_ip_port(value: &str) -> Option<String> {
    let (candidate_ip, port) = split_ip_port(value);
    let normalized_ip = normalize_malformed_ipv4(parse_ipv4_octets(candidate_ip)?)?;
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

struct ParsedIpv4 {
    remainders: [u16; 4],
    in_range: bool,
}

fn normalize_malformed_ipv4(octets: ParsedIpv4) -> Option<String> {
    if octets.in_range {
        return None;
    }

    let [a, b, c, d] = octets.remainders;
    Some(format!("{a}.{b}.{c}.{d}"))
}

fn parse_ipv4_octets(value: &str) -> Option<ParsedIpv4> {
    let mut remainders = [0u16; 4];
    let mut in_range = true;
    let mut parts = value.split('.');
    for remainder in &mut remainders {
        let part = parts.next()?;
        if part.is_empty() {
            return None;
        }
        let mut bounded = 0u16;
        for byte in part.bytes() {
            if !byte.is_ascii_digit() {
                return None;
            }
            let digit = u16::from(byte - b'0');
            *remainder = (*remainder * 10 + digit) % 255;
            // Retain the valid 255 boundary without accumulating an unbounded integer.
            bounded = (bounded * 10 + digit).min(256);
        }
        in_range &= bounded <= 255;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(ParsedIpv4 { remainders, in_range })
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
    use std::io::Read;

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
        assert_eq!(result.transformed_fields, 5);
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
                .contains(&format!("\"id\":\"{}\"", v::MALFORMED_HTTP_CLIENT_ID))
        );
    }

    #[test]
    fn normalizes_supported_reader_to_temp_without_value_materialization() {
        let input = format!(
            r#"{{
            "nodes": {{
                "a": {{
                    "ip": "{malformed}",
                    "transport_address": "{malformed_port}",
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
            malformed_client_id = v::MALFORMED_HTTP_CLIENT_ID,
        );

        let mut result = normalize_supported_reader_to_temp("diag/nodes.json", input.as_bytes()).expect("normalize");
        let mut content = String::new();
        result
            .file
            .as_file_mut()
            .read_to_string(&mut content)
            .expect("read normalized temp");

        assert_eq!(result.transformed_fields, 2);
        assert!(content.contains(&format!("\"{}\"", v::NORMALIZED_IP)));
        assert!(content.contains(&format!("\"{}\"", v::NORMALIZED_IP_WITH_PORT)));
        let doc: Value = serde_json::from_str(&content).expect("parse normalized JSON");
        assert_eq!(
            doc["nodes"]["a"]["http"]["clients"][0]["id"],
            v::MALFORMED_HTTP_CLIENT_ID
        );
    }

    #[test]
    fn raw_and_streaming_normalization_preserve_field_scope_through_arrays() {
        let input = serde_json::json!({
            "bound_address": [
                v::MALFORMED_IP_WITH_PORT,
                [v::MALFORMED_IP_SECONDARY_WITH_PORT],
                {"name": v::MALFORMED_IP, "host": v::MALFORMED_IP}
            ],
            "host": {"name": v::MALFORMED_IP},
            "name": [v::MALFORMED_IP],
            "http": {"clients": [{"id": v::MALFORMED_HTTP_CLIENT_ID}]}
        });
        let expected = serde_json::json!({
            "bound_address": [
                v::NORMALIZED_IP_WITH_PORT,
                [v::NORMALIZED_IP_SECONDARY_WITH_PORT],
                {"name": v::MALFORMED_IP, "host": v::NORMALIZED_IP}
            ],
            "host": {"name": v::MALFORMED_IP},
            "name": [v::MALFORMED_IP],
            "http": {"clients": [{"id": v::MALFORMED_HTTP_CLIENT_ID}]}
        });
        let input = input.to_string();
        let mut streamed = normalize_supported_reader_to_temp("nodes.json", input.as_bytes()).unwrap();
        let raw = normalize_supported_content("nodes.json", input).unwrap();
        assert_eq!(
            serde_json::from_reader::<_, Value>(streamed.file.as_file_mut()).unwrap(),
            expected
        );
        assert_eq!(serde_json::from_str::<Value>(&raw.content).unwrap(), expected);
    }

    #[test]
    fn raw_and_streaming_normalization_handle_unbounded_decimal_octets() {
        // Concatenated multiples of 255 remain divisible by 255 at any length.
        let huge = format!("{}.255.256.1", "255".repeat(128));
        for (address, expected_address) in [
            ("65536.1.2.3", "1.1.2.3"),
            ("18446744073709551616.255.256.1", "1.0.1.1"),
            (huge.as_str(), "0.0.1.1"),
            ("000255.255.0.1", "000255.255.0.1"),
        ] {
            let input = serde_json::json!({
                "ip": address,
                "host": format!("{address}:9300"),
                "transport_address": format!("{address}:9300"),
                "name": address,
                "http": {"clients": [{"id": address}]}
            })
            .to_string();
            let expected = serde_json::json!({
                "ip": expected_address,
                "host": expected_address,
                "transport_address": format!("{expected_address}:9300"),
                "name": address,
                "http": {"clients": [{"id": address}]}
            });
            let mut streamed = normalize_supported_reader_to_temp("nodes.json", input.as_bytes()).unwrap();
            let raw = normalize_supported_content("nodes.json", input).unwrap();
            assert_eq!(
                serde_json::from_reader::<_, Value>(streamed.file.as_file_mut()).unwrap(),
                expected,
                "streamed address: {address}"
            );
            assert_eq!(
                serde_json::from_str::<Value>(&raw.content).unwrap(),
                expected,
                "raw address: {address}"
            );
        }
    }

    #[test]
    fn raw_and_streaming_normalization_preserve_invalid_address_structure() {
        let expected = serde_json::json!({
            "ip": [
                "65536.1.2",
                "65536.1.2.3.4",
                "65536..2.3",
                "-65536.1.2.3",
                "+65536.1.2.3",
                "65536.1.2.３",
                "node-65536.1.2.3",
                "65536.1.2.3:not-a-port",
                "2001:db8::1"
            ]
        });
        let input = expected.to_string();
        let mut streamed = normalize_supported_reader_to_temp("nodes.json", input.as_bytes()).unwrap();
        let raw = normalize_supported_content("nodes.json", input).unwrap();
        assert_eq!(
            serde_json::from_reader::<_, Value>(streamed.file.as_file_mut()).unwrap(),
            expected
        );
        assert_eq!(serde_json::from_str::<Value>(&raw.content).unwrap(), expected);
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
