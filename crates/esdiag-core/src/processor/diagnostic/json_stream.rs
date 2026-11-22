// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use eyre::{Result, eyre};
use serde::de::DeserializeOwned;
use std::io::BufRead;
use tokio::sync::mpsc::Sender;

/// Framing helpers that scan a buffered JSON stream for top-level collection
/// entries, copy each entry's raw bytes into a single reusable buffer, and
/// deserialize with `serde_json::from_slice`. This gives `SliceRead` speed
/// without holding multi-gigabyte JSON files in memory.
struct JsonScanner<R> {
    reader: R,
}

impl<R: BufRead> JsonScanner<R> {
    fn new(reader: R) -> Self {
        Self { reader }
    }

    /// Skip ASCII JSON whitespace and return the next byte without consuming it,
    /// or `None` on EOF.
    fn peek_non_ws(&mut self) -> Result<Option<u8>> {
        loop {
            let buf = self.reader.fill_buf()?;
            if buf.is_empty() {
                return Ok(None);
            }
            if let Some(pos) = buf.iter().position(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n')) {
                let byte = buf[pos];
                if pos > 0 {
                    self.reader.consume(pos);
                }
                return Ok(Some(byte));
            }
            let len = buf.len();
            self.reader.consume(len);
        }
    }

    /// Consume the next single byte (caller must ensure it is available via peek).
    fn consume_byte(&mut self) -> Result<u8> {
        let buf = self.reader.fill_buf()?;
        if buf.is_empty() {
            return Err(eyre!("Unexpected EOF in JSON stream"));
        }
        let b = buf[0];
        self.reader.consume(1);
        Ok(b)
    }

    fn expect_byte(&mut self, expected: u8) -> Result<()> {
        match self.peek_non_ws()? {
            Some(b) if b == expected => {
                self.reader.consume(1);
                Ok(())
            }
            Some(other) => Err(eyre!(
                "Expected '{}' in JSON stream, found '{}'",
                expected as char,
                other as char
            )),
            None => Err(eyre!("Unexpected EOF while expecting '{}'", expected as char)),
        }
    }

    /// Read a JSON string (including surrounding quotes) into `out`.
    fn read_string_raw(&mut self, out: &mut Vec<u8>) -> Result<()> {
        self.expect_byte(b'"')?;
        out.push(b'"');
        let mut escaped = false;
        loop {
            let buf = self.reader.fill_buf()?;
            if buf.is_empty() {
                return Err(eyre!("Unexpected EOF inside JSON string"));
            }
            let mut i = 0;
            while i < buf.len() {
                let b = buf[i];
                if escaped {
                    escaped = false;
                    i += 1;
                } else if b == b'\\' {
                    escaped = true;
                    i += 1;
                } else if b == b'"' {
                    out.extend_from_slice(&buf[..=i]);
                    self.reader.consume(i + 1);
                    return Ok(());
                } else {
                    i += 1;
                }
            }
            out.extend_from_slice(buf);
            let len = buf.len();
            self.reader.consume(len);
        }
    }

    /// Read a JSON string and decode it via `serde_json::from_slice`.
    fn read_string(&mut self, scratch: &mut Vec<u8>) -> Result<String> {
        scratch.clear();
        self.read_string_raw(scratch)?;
        let s: String = serde_json::from_slice(scratch)?;
        Ok(s)
    }

    /// Capture one complete JSON value (object, array, string, number, bool, null)
    /// into `out`. If `out` is `None`, the value is skipped without allocating.
    fn scan_value(&mut self, mut out: Option<&mut Vec<u8>>) -> Result<()> {
        let first = match self.peek_non_ws()? {
            Some(b) => b,
            None => return Err(eyre!("Unexpected EOF while expecting JSON value")),
        };

        match first {
            b'"' => {
                if let Some(buf) = out {
                    self.read_string_raw(buf)
                } else {
                    self.skip_string()
                }
            }
            b'{' | b'[' => {
                let mut depth: usize = 0;
                let mut in_string = false;
                let mut escaped = false;
                loop {
                    let buf = self.reader.fill_buf()?;
                    if buf.is_empty() {
                        return Err(eyre!("Unexpected EOF inside JSON container"));
                    }
                    let mut i = 0;
                    while i < buf.len() {
                        let b = buf[i];
                        if in_string {
                            if escaped {
                                escaped = false;
                            } else if b == b'\\' {
                                escaped = true;
                            } else if b == b'"' {
                                in_string = false;
                            }
                            i += 1;
                        } else {
                            match b {
                                b'"' => {
                                    in_string = true;
                                    i += 1;
                                }
                                b'{' | b'[' => {
                                    depth += 1;
                                    i += 1;
                                }
                                b'}' | b']' => {
                                    depth -= 1;
                                    i += 1;
                                    if depth == 0 {
                                        if let Some(dest) = out.as_deref_mut() {
                                            dest.extend_from_slice(&buf[..i]);
                                        }
                                        self.reader.consume(i);
                                        return Ok(());
                                    }
                                }
                                _ => {
                                    i += 1;
                                }
                            }
                        }
                    }
                    if let Some(dest) = out.as_deref_mut() {
                        dest.extend_from_slice(buf);
                    }
                    let len = buf.len();
                    self.reader.consume(len);
                }
            }
            _ => {
                // Primitive: number, true, false, null. Scan until delimiter or whitespace.
                loop {
                    let buf = self.reader.fill_buf()?;
                    if buf.is_empty() {
                        return Ok(());
                    }
                    if let Some(pos) = buf
                        .iter()
                        .position(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b',' | b'}' | b']'))
                    {
                        if let Some(dest) = out.as_deref_mut() {
                            dest.extend_from_slice(&buf[..pos]);
                        }
                        self.reader.consume(pos);
                        return Ok(());
                    }
                    if let Some(dest) = out.as_deref_mut() {
                        dest.extend_from_slice(buf);
                    }
                    let len = buf.len();
                    self.reader.consume(len);
                }
            }
        }
    }

    fn skip_string(&mut self) -> Result<()> {
        self.expect_byte(b'"')?;
        let mut escaped = false;
        loop {
            let buf = self.reader.fill_buf()?;
            if buf.is_empty() {
                return Err(eyre!("Unexpected EOF inside JSON string"));
            }
            let mut i = 0;
            while i < buf.len() {
                let b = buf[i];
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    self.reader.consume(i + 1);
                    return Ok(());
                }
                i += 1;
            }
            let len = buf.len();
            self.reader.consume(len);
        }
    }
}

/// Stream entries `(String, V)` from a top-level JSON object `{ "k1": v1, "k2": v2, ... }`.
/// Each entry value is framed into a reusable buffer and parsed with `serde_json::from_slice`.
pub fn stream_root_map<R, V>(reader: R, sender: Sender<Result<(String, V)>>) -> Result<()>
where
    R: BufRead,
    V: DeserializeOwned + Send + 'static,
{
    let mut scanner = JsonScanner::new(reader);
    stream_map_body(&mut scanner, &sender)
}

/// Stream entries `(String, V)` from a named map field inside a top-level JSON object:
/// `{ ..., "<field>": { "k1": v1, ... }, ... }`.
pub fn stream_nested_map<R, V>(reader: R, field: &str, sender: Sender<Result<(String, V)>>) -> Result<()>
where
    R: BufRead,
    V: DeserializeOwned + Send + 'static,
{
    let mut scanner = JsonScanner::new(reader);
    scanner.expect_byte(b'{')?;
    let mut key_buf = Vec::with_capacity(64);
    let mut first = true;
    loop {
        match scanner.peek_non_ws()? {
            Some(b'}') => {
                scanner.consume_byte()?;
                return Ok(());
            }
            Some(b',') if !first => {
                scanner.consume_byte()?;
            }
            Some(_) if first => {}
            other => {
                return Err(eyre!("Unexpected token in root object: {:?}", other));
            }
        }
        first = false;
        let key = scanner.read_string(&mut key_buf)?;
        scanner.expect_byte(b':')?;
        if key == field {
            stream_map_body(&mut scanner, &sender)?;
        } else {
            scanner.scan_value(None)?;
        }
    }
}

/// Stream elements `V` from a named array field inside a top-level JSON object:
/// `{ ..., "<field>": [ v1, v2, ... ], ... }`.
/// Each element is parsed independently; item-level parse errors are sent through `sender`
/// without aborting the stream (matching `Snapshots::deserialize_stream`).
pub fn stream_nested_array<R, V>(reader: R, field: &str, sender: Sender<Result<V>>) -> Result<()>
where
    R: BufRead,
    V: DeserializeOwned + Send + 'static,
{
    let mut scanner = JsonScanner::new(reader);
    scanner.expect_byte(b'{')?;
    let mut key_buf = Vec::with_capacity(64);
    let mut first = true;
    loop {
        match scanner.peek_non_ws()? {
            Some(b'}') => {
                scanner.consume_byte()?;
                return Ok(());
            }
            Some(b',') if !first => {
                scanner.consume_byte()?;
            }
            Some(_) if first => {}
            other => {
                return Err(eyre!("Unexpected token in root object: {:?}", other));
            }
        }
        first = false;
        let key = scanner.read_string(&mut key_buf)?;
        scanner.expect_byte(b':')?;
        if key == field {
            stream_array_body(&mut scanner, &sender)?;
        } else {
            scanner.scan_value(None)?;
        }
    }
}

fn stream_map_body<R, V>(scanner: &mut JsonScanner<R>, sender: &Sender<Result<(String, V)>>) -> Result<()>
where
    R: BufRead,
    V: DeserializeOwned + Send + 'static,
{
    scanner.expect_byte(b'{')?;
    let mut key_buf = Vec::with_capacity(128);
    let mut val_buf = Vec::with_capacity(64 * 1024);
    let mut first = true;
    let mut sender_closed = false;

    loop {
        match scanner.peek_non_ws()? {
            Some(b'}') => {
                scanner.consume_byte()?;
                return Ok(());
            }
            Some(b',') if !first => {
                scanner.consume_byte()?;
            }
            Some(_) if first => {}
            other => {
                return Err(eyre!("Unexpected token in JSON map: {:?}", other));
            }
        }
        first = false;
        let key = scanner.read_string(&mut key_buf)?;
        scanner.expect_byte(b':')?;

        if sender_closed {
            scanner.scan_value(None)?;
            continue;
        }

        val_buf.clear();
        scanner.scan_value(Some(&mut val_buf))?;
        let value: V = serde_json::from_slice(&val_buf)?;
        if sender.blocking_send(Ok((key, value))).is_err() {
            sender_closed = true;
        }
        // Shrink if an abnormally huge single entry inflated the reusable buffer.
        if val_buf.capacity() > 16 * 1024 * 1024 {
            val_buf = Vec::with_capacity(64 * 1024);
        }
    }
}

fn stream_array_body<R, V>(scanner: &mut JsonScanner<R>, sender: &Sender<Result<V>>) -> Result<()>
where
    R: BufRead,
    V: DeserializeOwned + Send + 'static,
{
    scanner.expect_byte(b'[')?;
    let mut val_buf = Vec::with_capacity(64 * 1024);
    let mut first = true;
    let mut sender_closed = false;

    loop {
        match scanner.peek_non_ws()? {
            Some(b']') => {
                scanner.consume_byte()?;
                return Ok(());
            }
            Some(b',') if !first => {
                scanner.consume_byte()?;
            }
            Some(_) if first => {}
            other => {
                return Err(eyre!("Unexpected token in JSON array: {:?}", other));
            }
        }
        first = false;

        if sender_closed {
            scanner.scan_value(None)?;
            continue;
        }

        val_buf.clear();
        scanner.scan_value(Some(&mut val_buf))?;
        match serde_json::from_slice::<V>(&val_buf) {
            Ok(item) => {
                if sender.blocking_send(Ok(item)).is_err() {
                    sender_closed = true;
                }
            }
            Err(err) => {
                if sender.blocking_send(Err(eyre!(err))).is_err() {
                    sender_closed = true;
                }
            }
        }
        if val_buf.capacity() > 16 * 1024 * 1024 {
            val_buf = Vec::with_capacity(64 * 1024);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Entry {
        name: String,
        count: u64,
    }

    #[test]
    fn stream_root_map_handles_escaped_strings_and_nested_braces() {
        let json = br#"{
            "a": {"name": "hello \" } world", "count": 1},
            "b": {"name": "nested { [ ] }", "count": 2}
        }"#;
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        stream_root_map(std::io::Cursor::new(&json[..]), tx).unwrap();
        let mut items = Vec::new();
        while let Ok(item) = rx.try_recv() {
            items.push(item.unwrap());
        }
        assert_eq!(
            items,
            vec![
                (
                    "a".to_string(),
                    Entry {
                        name: "hello \" } world".to_string(),
                        count: 1
                    }
                ),
                (
                    "b".to_string(),
                    Entry {
                        name: "nested { [ ] }".to_string(),
                        count: 2
                    }
                ),
            ]
        );
    }

    #[test]
    fn stream_nested_map_skips_sibling_fields() {
        let json = br#"{
            "_shards": {"total": 10, "successful": 10},
            "indices": {
                "idx-1": {"name": "one", "count": 42},
                "idx-2": {"name": "two", "count": 99}
            },
            "trailing": [1, 2, {"a": "b"}]
        }"#;
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        stream_nested_map::<_, Entry>(std::io::Cursor::new(&json[..]), "indices", tx).unwrap();
        let mut items = Vec::new();
        while let Ok(item) = rx.try_recv() {
            items.push(item.unwrap());
        }
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].0, "idx-1");
        assert_eq!(items[1].1.count, 99);
    }

    #[test]
    fn stream_nested_array_emits_item_errors_without_aborting() {
        let json = br#"{
            "snapshots": [
                {"name": "ok-1", "count": 1},
                {"name": "bad", "count": "not-a-number"},
                {"name": "ok-2", "count": 2}
            ]
        }"#;
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        stream_nested_array::<_, Entry>(std::io::Cursor::new(&json[..]), "snapshots", tx).unwrap();
        let mut results = Vec::new();
        while let Ok(item) = rx.try_recv() {
            results.push(item);
        }
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].as_ref().unwrap().name, "ok-1");
        assert!(results[1].is_err());
        assert_eq!(results[2].as_ref().unwrap().name, "ok-2");
    }
}
