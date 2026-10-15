// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use eyre::{Result, bail};
use serde::Deserialize;
#[cfg(test)]
use serde_json::Value;
use std::collections::HashMap;
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
const MAX_NORMALIZATION_DEPTH: usize = 128;

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

    // Validate the complete input without materializing a Value tree.
    let mut validator = serde_json::Deserializer::from_str(&input);
    serde::de::IgnoredAny::deserialize(&mut validator)?;
    validator.end()?;

    let mut output = RawOutput::default();
    walk_json_fields(input.as_bytes(), &mut output)?;
    let (content, transformed_fields) = output.finish()?;
    Ok(TransformResult {
        content,
        transformed_fields,
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

    let transformed_fields;
    {
        let mut writer = BufWriter::new(file.as_file_mut());
        let mut output = TempOutput {
            writer: &mut writer,
            transformed_fields: 0,
        };
        walk_json_fields(reader, &mut output)?;
        transformed_fields = output.transformed_fields;
        writer.flush()?;
    }
    file.as_file_mut().seek(SeekFrom::Start(0))?;

    Ok(TempTransformResult {
        file,
        transformed_fields,
    })
}

// Only output and acceptance differ; decoded field scope belongs to the walk.
trait JsonOutput {
    fn whitespace(&mut self, byte: u8) -> Result<()>;
    fn key(&mut self, raw: &str, decoded: &str) -> Result<()>;
    fn string(&mut self, raw: &str, updated: Option<&str>) -> Result<()>;
    fn literal_byte(&mut self, byte: u8) -> Result<()>;
    fn literal_end(&mut self) -> Result<()>;
    fn begin_object(&mut self) -> Result<()>;
    fn begin_array(&mut self) -> Result<()>;
    fn end_object(&mut self) -> Result<()>;
    fn end_array(&mut self) -> Result<()>;
    fn punctuation(&mut self, byte: u8) -> Result<()>;
}

struct TempOutput<'a, W: Write> {
    writer: &'a mut W,
    transformed_fields: usize,
}

impl<W: Write> JsonOutput for TempOutput<'_, W> {
    fn whitespace(&mut self, byte: u8) -> Result<()> {
        self.writer.write_all(&[byte])?;
        Ok(())
    }
    fn key(&mut self, raw: &str, _decoded: &str) -> Result<()> {
        write!(self.writer, "\"{raw}\"")?;
        Ok(())
    }
    fn string(&mut self, raw: &str, updated: Option<&str>) -> Result<()> {
        if let Some(updated) = updated {
            serde_json::to_writer(&mut self.writer, updated)?;
            self.transformed_fields += 1;
        } else {
            write!(self.writer, "\"{raw}\"")?;
        }
        Ok(())
    }
    fn literal_byte(&mut self, byte: u8) -> Result<()> {
        self.writer.write_all(&[byte])?;
        Ok(())
    }
    fn literal_end(&mut self) -> Result<()> {
        Ok(())
    }
    fn begin_object(&mut self) -> Result<()> {
        self.punctuation(b'{')
    }
    fn begin_array(&mut self) -> Result<()> {
        self.punctuation(b'[')
    }
    fn end_object(&mut self) -> Result<()> {
        self.punctuation(b'}')
    }
    fn end_array(&mut self) -> Result<()> {
        self.punctuation(b']')
    }
    fn punctuation(&mut self, byte: u8) -> Result<()> {
        self.writer.write_all(&[byte])?;
        Ok(())
    }
}

// Fragments move into their parent; duplicate keys drop overwritten subtrees.
#[derive(Default)]
struct RawOutput {
    stack: Vec<RawContainer>,
    root: Option<RawFragment>,
    literal: Vec<u8>,
}

enum RawContainer {
    Object {
        entries: Vec<(String, RawFragment)>,
        indexes: HashMap<String, usize>,
        pending_key: Option<String>,
    },
    Array(Vec<RawFragment>),
}

enum RawFragment {
    Text { content: String, transformed: bool },
    Object(Vec<(String, RawFragment)>),
    Array(Vec<RawFragment>),
}

impl RawOutput {
    fn push(&mut self, fragment: RawFragment) {
        match self.stack.last_mut() {
            Some(RawContainer::Array(items)) => items.push(fragment),
            Some(RawContainer::Object {
                entries,
                indexes,
                pending_key,
            }) => {
                let key = pending_key.take().expect("validated object has a key");
                if let Some(&index) = indexes.get(&key) {
                    entries[index].1 = fragment;
                } else {
                    indexes.insert(key.clone(), entries.len());
                    entries.push((key, fragment));
                }
            }
            None => self.root = Some(fragment),
        }
    }
    fn begin_container(&mut self, container: RawContainer) -> Result<()> {
        // serde_json::Value rejects the 128th nested container.
        if self.stack.len() >= 127 {
            bail!("recursion limit exceeded");
        }
        self.stack.push(container);
        Ok(())
    }
    fn finish(self) -> Result<(String, usize)> {
        let root = self.root.ok_or_else(|| eyre::eyre!("missing JSON value"))?;
        let mut content = String::new();
        let mut transformed_fields = 0;
        root.append_to(&mut content, &mut transformed_fields)?;
        Ok((content, transformed_fields))
    }
}

impl RawFragment {
    fn append_to(self, output: &mut String, transformed: &mut usize) -> Result<()> {
        match self {
            Self::Text {
                content,
                transformed: changed,
            } => {
                output.push_str(&content);
                *transformed += usize::from(changed);
            }
            Self::Array(items) => {
                output.push('[');
                for (index, item) in items.into_iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    item.append_to(output, transformed)?;
                }
                output.push(']');
            }
            Self::Object(entries) => {
                output.push('{');
                for (index, (key, value)) in entries.into_iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    output.push_str(&serde_json::to_string(&key)?);
                    output.push(':');
                    value.append_to(output, transformed)?;
                }
                output.push('}');
            }
        }
        Ok(())
    }
}

impl JsonOutput for RawOutput {
    fn whitespace(&mut self, _byte: u8) -> Result<()> {
        Ok(())
    }
    fn key(&mut self, _raw: &str, decoded: &str) -> Result<()> {
        if let Some(RawContainer::Object { pending_key, .. }) = self.stack.last_mut() {
            *pending_key = Some(decoded.to_owned());
        }
        Ok(())
    }
    fn string(&mut self, raw: &str, updated: Option<&str>) -> Result<()> {
        self.push(RawFragment::Text {
            content: match updated {
                Some(value) => serde_json::to_string(value)?,
                None => format!("\"{raw}\""),
            },
            transformed: updated.is_some(),
        });
        Ok(())
    }
    fn literal_byte(&mut self, byte: u8) -> Result<()> {
        self.literal.push(byte);
        Ok(())
    }
    fn literal_end(&mut self) -> Result<()> {
        let raw = String::from_utf8(std::mem::take(&mut self.literal))?;
        let content = if raw.as_bytes()[0] == b'-' || raw.as_bytes()[0].is_ascii_digit() {
            serde_json::from_str::<serde_json::Number>(&raw)?.to_string()
        } else {
            raw
        };
        self.push(RawFragment::Text {
            content,
            transformed: false,
        });
        Ok(())
    }
    fn begin_object(&mut self) -> Result<()> {
        self.begin_container(RawContainer::Object {
            entries: Vec::new(),
            indexes: HashMap::new(),
            pending_key: None,
        })
    }
    fn begin_array(&mut self) -> Result<()> {
        self.begin_container(RawContainer::Array(Vec::new()))
    }
    fn end_object(&mut self) -> Result<()> {
        if let Some(RawContainer::Object { entries, .. }) = self.stack.pop() {
            self.push(RawFragment::Object(entries));
        }
        Ok(())
    }
    fn end_array(&mut self) -> Result<()> {
        if let Some(RawContainer::Array(items)) = self.stack.pop() {
            self.push(RawFragment::Array(items));
        }
        Ok(())
    }
    fn punctuation(&mut self, _byte: u8) -> Result<()> {
        Ok(())
    }
}

fn walk_json_fields<R: BufRead, O: JsonOutput>(reader: R, output: &mut O) -> Result<()> {
    let mut bytes = reader.bytes().peekable();
    let mut stack = Vec::<JsonContext>::new();
    while let Some(byte) = bytes.next() {
        let byte = byte?;
        match byte {
            b'{' | b'[' if stack.len() >= MAX_NORMALIZATION_DEPTH => {
                eyre::bail!("Scrub normalization exceeds the {MAX_NORMALIZATION_DEPTH}-container nesting limit");
            }
            b'"' => {
                let raw = read_json_string_body(&mut bytes)?;
                if is_object_key_context(&stack) {
                    let key = serde_json::from_str::<String>(&format!("\"{raw}\""))?;
                    output.key(&raw, &key)?;
                    if let Some(JsonContext::Object { pending_key, .. }) = stack.last_mut() {
                        *pending_key = Some(key);
                    }
                } else {
                    let value = serde_json::from_str::<String>(&format!("\"{raw}\""))?;
                    let updated = normalize_string_for_field(field_for_value(&stack), &value).filter(|s| *s != value);
                    output.string(&raw, updated.as_deref())?;
                    mark_value_written(&mut stack);
                }
            }
            b'{' => {
                output.begin_object()?;
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
                output.begin_array()?;
                stack.push(JsonContext::Array { field });
            }
            b'}' => {
                output.end_object()?;
                stack.pop();
                mark_value_written(&mut stack);
            }
            b']' => {
                output.end_array()?;
                stack.pop();
                mark_value_written(&mut stack);
            }
            b',' => {
                output.punctuation(b',')?;
                if let Some(JsonContext::Object {
                    pending_key,
                    expecting_key,
                }) = stack.last_mut()
                {
                    *pending_key = None;
                    *expecting_key = true;
                }
            }
            b':' => {
                output.punctuation(b':')?;
                if let Some(JsonContext::Object { expecting_key, .. }) = stack.last_mut() {
                    *expecting_key = false;
                }
            }
            byte if byte.is_ascii_whitespace() => output.whitespace(byte)?,
            byte => {
                output.literal_byte(byte)?;
                copy_literal_tail(&mut bytes, output)?;
                output.literal_end()?;
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
            return Ok(String::from_utf8(raw)?);
        } else {
            raw.push(byte);
        }
    }
    eyre::bail!("Unterminated JSON string")
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

fn copy_literal_tail<I, O>(bytes: &mut std::iter::Peekable<I>, output: &mut O) -> Result<()>
where
    I: Iterator<Item = std::io::Result<u8>>,
    O: JsonOutput,
{
    while let Some(byte) = bytes.peek() {
        let byte = match byte {
            Ok(byte) => *byte,
            Err(_) => break,
        };
        if byte == b',' || byte == b'}' || byte == b']' || byte.is_ascii_whitespace() {
            break;
        }
        output.literal_byte(bytes.next().transpose()?.expect("peeked byte"))?;
    }
    Ok(())
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
    use std::io::{BufReader, Read};

    #[test]
    fn normalized_reader_bounds_container_depth_before_consuming_the_payload() {
        for (open, close) in [("[", "]"), ("{\"child\":", "}")] {
            let input = format!("{}0{}", open.repeat(128), close.repeat(128));
            let result = with_normalized_json_reader("nodes.json", input.as_bytes(), true, |reader| {
                serde_json::from_reader::<_, Box<serde_json::value::RawValue>>(reader).map_err(Into::into)
            })
            .expect("128 containers remain supported for a RawValue consumer");
            assert_eq!(result.get(), input);

            let input = format!("{}0{}", open.repeat(129), close.repeat(129));
            let mut reader = std::io::Cursor::new(input.as_bytes());
            let result = with_normalized_json_reader("nodes.json", &mut reader, true, |reader| {
                serde_json::from_reader::<_, Box<serde_json::value::RawValue>>(reader).map_err(Into::into)
            });
            assert!(result.is_err(), "accepted more than 128 containers");
            assert!(
                reader.position() <= (open.len() * 129) as u64,
                "read the payload beyond the first over-budget container"
            );
        }
    }

    #[test]
    fn normalization_depth_budget_does_not_limit_direct_readers() {
        let input = format!("{}0{}", "[".repeat(512), "]".repeat(512));
        for (path, scrubbed) in [
            ("nodes.json", false),
            ("version.json", true),
            ("diagnostic_manifest.json", true),
            ("tasks.txt", true),
        ] {
            let result = with_normalized_json_reader(path, input.as_bytes(), scrubbed, |reader| {
                serde_json::from_reader::<_, Box<serde_json::value::RawValue>>(reader).map_err(Into::into)
            })
            .expect("direct reader retains its consumer's depth acceptance");
            assert_eq!(result.get(), input);
        }
    }

    #[test]
    fn normalized_reader_rejects_unterminated_strings() {
        for input in [b"\"truncated".as_slice(), b"\"trailing\\", b"\"escaped\\\""] {
            let result = with_normalized_json_reader("nodes.json", input, true, |reader| {
                serde_json::from_reader::<_, String>(reader).map_err(Into::into)
            });
            assert!(result.is_err(), "accepted truncated JSON: {input:?}");
        }

        let result = with_normalized_json_reader("nodes.json", br#""complete\"string""#.as_slice(), true, |reader| {
            serde_json::from_reader::<_, String>(reader).map_err(Into::into)
        })
        .unwrap();
        assert_eq!(result, "complete\"string");
    }

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
    fn reader_errors_propagate_without_consuming_partial_temp_output() {
        struct FailingReader;
        impl Read for FailingReader {
            fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("synthetic read failure"))
            }
        }
        let prefix = std::io::Cursor::new(br#"{"ip":"512.768.1024.1280"}"#);
        let reader = BufReader::new(prefix.chain(FailingReader));
        let result = with_normalized_json_reader("nodes.json", reader, true, |_reader| -> Result<()> {
            panic!("a partial temporary file must not reach the consumer")
        });
        assert!(result.unwrap_err().to_string().contains("synthetic read failure"));
    }

    #[test]
    fn raw_duplicate_counts_only_surviving_decoded_keys_and_subtrees() {
        let input = r#"{"ip":"512.768.1024.1280","\u0069p":"192.0.2.10","host":"512.768.1024.1280","host":"513.769.1025.1281:9300","nested":{"ip":"512.768.1024.1280"},"nested":{"name":"café","ip":"192.0.2.11"},"bound_address":["512.768.1024.1280:9300",{"ip":"512.768.1024.1280"}]}"#;
        let result = normalize_supported_content("nodes.json", input.into()).expect("raw normalization");
        assert_eq!(result.transformed_fields, 3);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["ip"], "192.0.2.10");
        assert_eq!(parsed["host"], "3.4.5.6");
        assert_eq!(parsed["nested"]["ip"], "192.0.2.11");
        assert_eq!(
            parsed["bound_address"],
            serde_json::json!(["2.3.4.5:9300", {"ip":"2.3.4.5"}])
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
