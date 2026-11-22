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
    /// into `out`.
    fn capture_value(&mut self, out: &mut Vec<u8>) -> Result<()> {
        let first = match self.peek_non_ws()? {
            Some(b) => b,
            None => return Err(eyre!("Unexpected EOF while expecting JSON value")),
        };

        match first {
            b'"' => self.read_string_raw(out),
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
                                        out.extend_from_slice(&buf[..i]);
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
                    out.extend_from_slice(buf);
                    let len = buf.len();
                    self.reader.consume(len);
                }
            }
            b',' | b'}' | b']' | b':' => Err(eyre!("Unexpected token '{}' while expecting JSON value", first as char)),
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
                        out.extend_from_slice(&buf[..pos]);
                        self.reader.consume(pos);
                        return Ok(());
                    }
                    out.extend_from_slice(buf);
                    let len = buf.len();
                    self.reader.consume(len);
                }
            }
        }
    }

    /// Skip and validate one complete JSON value with bounded memory without
    /// over-reading past the value boundary in the underlying `BufRead`.
    fn skip_value(&mut self) -> Result<()> {
        let mut depth: usize = 0;
        self.skip_value_recursive(&mut depth)
    }

    fn skip_value_recursive(&mut self, depth: &mut usize) -> Result<()> {
        const MAX_DEPTH: usize = 128;
        let first = match self.peek_non_ws()? {
            Some(b) => b,
            None => return Err(eyre!("Unexpected EOF while expecting JSON value")),
        };
        match first {
            b'{' => {
                *depth += 1;
                if *depth > MAX_DEPTH {
                    return Err(eyre!("JSON nesting depth exceeded while skipping value"));
                }
                self.consume_byte()?;
                let mut first_field = true;
                loop {
                    match self.peek_non_ws()? {
                        Some(b'}') => {
                            self.consume_byte()?;
                            *depth -= 1;
                            return Ok(());
                        }
                        Some(b',') if !first_field => {
                            self.consume_byte()?;
                            if matches!(self.peek_non_ws()?, Some(b'}')) {
                                return Err(eyre!("Trailing comma in skipped JSON object"));
                            }
                        }
                        Some(_) if first_field => {}
                        other => {
                            return Err(eyre!("Unexpected token in skipped object: {:?}", other));
                        }
                    }
                    first_field = false;
                    self.skip_validated_string()?;
                    self.expect_byte(b':')?;
                    self.skip_value_recursive(depth)?;
                }
            }
            b'[' => {
                *depth += 1;
                if *depth > MAX_DEPTH {
                    return Err(eyre!("JSON nesting depth exceeded while skipping value"));
                }
                self.consume_byte()?;
                let mut first_elem = true;
                loop {
                    match self.peek_non_ws()? {
                        Some(b']') => {
                            self.consume_byte()?;
                            *depth -= 1;
                            return Ok(());
                        }
                        Some(b',') if !first_elem => {
                            self.consume_byte()?;
                            if matches!(self.peek_non_ws()?, Some(b']')) {
                                return Err(eyre!("Trailing comma in skipped JSON array"));
                            }
                        }
                        Some(_) if first_elem => {}
                        other => {
                            return Err(eyre!("Unexpected token in skipped array: {:?}", other));
                        }
                    }
                    first_elem = false;
                    self.skip_value_recursive(depth)?;
                }
            }
            b'"' => self.skip_validated_string(),
            b',' | b'}' | b']' | b':' => Err(eyre!("Unexpected token '{}' while expecting JSON value", first as char)),
            _ => {
                let mut prim_buf = Vec::with_capacity(32);
                loop {
                    let buf = self.reader.fill_buf()?;
                    if buf.is_empty() {
                        break;
                    }
                    if let Some(pos) = buf
                        .iter()
                        .position(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b',' | b'}' | b']'))
                    {
                        prim_buf.extend_from_slice(&buf[..pos]);
                        self.reader.consume(pos);
                        break;
                    }
                    prim_buf.extend_from_slice(buf);
                    let len = buf.len();
                    self.reader.consume(len);
                    if prim_buf.len() > 1024 {
                        return Err(eyre!("Invalid JSON primitive while skipping value"));
                    }
                }
                serde_json::from_slice::<serde::de::IgnoredAny>(&prim_buf)?;
                Ok(())
            }
        }
    }

    /// Incrementally validate and discard a JSON string in O(1) memory,
    /// validating UTF-8, unescaped control characters, escape sequences,
    /// and UTF-16 surrogate pairs across arbitrary `BufRead` chunk boundaries.
    fn skip_validated_string(&mut self) -> Result<()> {
        self.expect_byte(b'"')?;
        let mut utf8_buf = [0u8; 4];
        let mut utf8_len: usize = 0;
        let mut utf8_expected: usize = 0;
        let mut pending_high_surrogate = false;

        loop {
            let b = self.consume_byte()?;
            if utf8_expected > 0 {
                if (b & 0xC0) != 0x80 {
                    return Err(eyre!("Invalid UTF-8 continuation byte in skipped JSON string"));
                }
                utf8_buf[utf8_len] = b;
                utf8_len += 1;
                if utf8_len == utf8_expected {
                    if std::str::from_utf8(&utf8_buf[..utf8_len]).is_err() {
                        return Err(eyre!("Invalid UTF-8 sequence in skipped JSON string"));
                    }
                    utf8_len = 0;
                    utf8_expected = 0;
                }
                continue;
            }

            if pending_high_surrogate {
                if b != b'\\' {
                    return Err(eyre!("Lone high surrogate in skipped JSON string"));
                }
                if self.consume_byte()? != b'u' {
                    return Err(eyre!("Lone high surrogate in skipped JSON string"));
                }
                let low = self.read_hex4()?;
                if !(0xDC00..=0xDFFF).contains(&low) {
                    return Err(eyre!("Invalid low surrogate in skipped JSON string"));
                }
                pending_high_surrogate = false;
                continue;
            }

            match b {
                b'"' => return Ok(()),
                b'\\' => match self.consume_byte()? {
                    b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {}
                    b'u' => {
                        let code = self.read_hex4()?;
                        if (0xD800..=0xDBFF).contains(&code) {
                            pending_high_surrogate = true;
                        } else if (0xDC00..=0xDFFF).contains(&code) {
                            return Err(eyre!("Lone low surrogate in skipped JSON string"));
                        }
                    }
                    other => {
                        return Err(eyre!("Invalid escape '\\{}' in skipped JSON string", other as char));
                    }
                },
                0x00..=0x1F => {
                    return Err(eyre!("Unescaped control character 0x{:02X} in skipped JSON string", b));
                }
                0x20..=0x7F => {}
                0xC2..=0xDF => {
                    utf8_buf[0] = b;
                    utf8_len = 1;
                    utf8_expected = 2;
                }
                0xE0..=0xEF => {
                    utf8_buf[0] = b;
                    utf8_len = 1;
                    utf8_expected = 3;
                }
                0xF0..=0xF4 => {
                    utf8_buf[0] = b;
                    utf8_len = 1;
                    utf8_expected = 4;
                }
                _ => {
                    return Err(eyre!("Invalid UTF-8 lead byte 0x{:02X} in skipped JSON string", b));
                }
            }
        }
    }

    fn read_hex4(&mut self) -> Result<u16> {
        let mut val: u16 = 0;
        for _ in 0..4 {
            let b = self.consume_byte()?;
            let digit = match b {
                b'0'..=b'9' => (b - b'0') as u16,
                b'a'..=b'f' => (b - b'a' + 10) as u16,
                b'A'..=b'F' => (b - b'A' + 10) as u16,
                _ => return Err(eyre!("Invalid hex digit '{}' in unicode escape", b as char)),
            };
            val = (val << 4) | digit;
        }
        Ok(val)
    }

    fn expect_eof(&mut self) -> Result<()> {
        match self.peek_non_ws()? {
            None => Ok(()),
            Some(other) => Err(eyre!(
                "Unexpected trailing byte '{}' after root JSON value",
                other as char
            )),
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
    stream_map_body(&mut scanner, &sender)?;
    scanner.expect_eof()
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
                return scanner.expect_eof();
            }
            Some(b',') if !first => {
                scanner.consume_byte()?;
                if matches!(scanner.peek_non_ws()?, Some(b'}')) {
                    return Err(eyre!("Trailing comma in root JSON object"));
                }
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
            scanner.skip_value()?;
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
                return scanner.expect_eof();
            }
            Some(b',') if !first => {
                scanner.consume_byte()?;
                if matches!(scanner.peek_non_ws()?, Some(b'}')) {
                    return Err(eyre!("Trailing comma in root JSON object"));
                }
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
            scanner.skip_value()?;
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
                if matches!(scanner.peek_non_ws()?, Some(b'}')) {
                    return Err(eyre!("Trailing comma in JSON map"));
                }
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
            scanner.skip_value()?;
            continue;
        }

        val_buf.clear();
        scanner.capture_value(&mut val_buf)?;
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
                if matches!(scanner.peek_non_ws()?, Some(b']')) {
                    return Err(eyre!("Trailing comma in JSON array"));
                }
            }
            Some(_) if first => {}
            other => {
                return Err(eyre!("Unexpected token in JSON array: {:?}", other));
            }
        }
        first = false;

        if sender_closed {
            scanner.skip_value()?;
            continue;
        }

        val_buf.clear();
        scanner.capture_value(&mut val_buf)?;
        match serde_json::from_slice::<V>(&val_buf) {
            Ok(item) => {
                if sender.blocking_send(Ok(item)).is_err() {
                    sender_closed = true;
                }
            }
            Err(err) => {
                // Only treat semantically invalid (type/data) frames as recoverable
                // per-item errors. Validate the entire frame as JSON syntax first so
                // malformed JSON (including syntax errors hidden after an early type
                // error) aborts the stream instead of exporting later entries.
                serde_json::from_slice::<serde::de::IgnoredAny>(&val_buf)?;
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

    #[test]
    fn stream_nested_map_rejects_malformed_skipped_values() {
        for bad in [
            br#"{"_shards":garbage,"indices":{}}"#.as_slice(),
            br#"{"_shards":[},"indices":{}}"#.as_slice(),
            br#"{"_shards":{"a":1,},"indices":{}}"#.as_slice(),
            br#"{"_shards":[1,],"indices":{}}"#.as_slice(),
            br#"{"_shards":"\uZZZZ","indices":{}}"#.as_slice(),
            br#"{"indices":{}} trailing"#.as_slice(),
        ] {
            let (tx, _rx) = tokio::sync::mpsc::channel::<Result<(String, Entry)>>(16);
            assert!(
                stream_nested_map(std::io::Cursor::new(bad), "indices", tx).is_err(),
                "expected error for malformed input: {}",
                String::from_utf8_lossy(bad)
            );
        }
    }

    #[test]
    fn stream_map_validates_entries_after_receiver_closes() {
        let json = br#"{
            "a": {"name": "ok", "count": 1},
            "b": {"name": "malformed", trailing}
        }"#;
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<(String, Entry)>>(1);
        drop(rx);
        assert!(stream_root_map(std::io::Cursor::new(&json[..]), tx).is_err());
    }

    #[test]
    fn stream_nested_array_aborts_on_syntax_errors_including_hidden_ones() {
        for bad in [
            // Trailing comma
            br#"{"snapshots":[{"name":"ok","count":1},]}"#.as_slice(),
            // Doubled comma (empty frame)
            br#"{"snapshots":[{"name":"ok","count":1},,{"name":"ok2","count":2}]}"#.as_slice(),
            // Syntax error inside frame
            br#"{"snapshots":[{"name":"ok","count":1},{bad},{"name":"ok2","count":2}]}"#.as_slice(),
            // Type error early in frame hiding a syntax error later in the same frame
            br#"{"snapshots":[{"name":123,"count":bad_token},{"name":"ok2","count":2}]}"#.as_slice(),
        ] {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<Entry>>(16);
            let res = stream_nested_array(std::io::Cursor::new(bad), "snapshots", tx);
            assert!(
                res.is_err(),
                "expected stream error for malformed array: {}",
                String::from_utf8_lossy(bad)
            );
            let mut exported_ok2 = false;
            while let Ok(item) = rx.try_recv() {
                if let Ok(entry) = item
                    && entry.name == "ok2"
                {
                    exported_ok2 = true;
                }
            }
            assert!(
                !exported_ok2,
                "subsequent entry must not be exported after syntax error: {}",
                String::from_utf8_lossy(bad)
            );
        }
    }

    #[test]
    fn skip_validated_string_handles_large_strings_and_escapes_across_small_chunks() {
        // Small-chunk BufReader (7 bytes) forces escapes, surrogate pairs, UTF-8,
        // and skipped object keys to split across buffer boundaries.
        let big_payload = "a".repeat(256 * 1024);
        let big_key = "k".repeat(64 * 1024);
        let valid = format!(
            r#"{{"skipped_obj":{{"{big_key}":"{big_payload}\n\t\"\\\u0041\uD83D\uDE00é🦀"}},"indices":{{"ok":{{"name":"kept","count":7}}}}}}"#
        );
        let reader = std::io::BufReader::with_capacity(7, std::io::Cursor::new(valid.as_bytes()));
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<(String, Entry)>>(16);
        stream_nested_map(reader, "indices", tx).unwrap();
        let item = rx.try_recv().unwrap().unwrap();
        assert_eq!(item.0, "ok");
        assert_eq!(item.1.count, 7);

        // Invalid escapes, control chars, lone surrogates, and bad UTF-8 across chunk boundaries must fail.
        for bad in [
            br#"{"skip":"abc\x20def","indices":{}}"#.as_slice(),
            b"{\"skip\":\"abc\ndef\",\"indices\":{}}".as_slice(),
            br#"{"skip":"\uD83Dabc","indices":{}}"#.as_slice(),
            br#"{"skip":"\uDE00","indices":{}}"#.as_slice(),
            b"{\"skip\":\"\xFF\xFE\",\"indices\":{}}".as_slice(),
            br#"{"skip_obj":{"bad\qkey":1},"indices":{}}"#.as_slice(),
        ] {
            let reader = std::io::BufReader::with_capacity(3, std::io::Cursor::new(bad));
            let (tx, _rx) = tokio::sync::mpsc::channel::<Result<(String, Entry)>>(16);
            assert!(
                stream_nested_map(reader, "indices", tx).is_err(),
                "expected error for malformed skipped string"
            );
        }
    }
}
