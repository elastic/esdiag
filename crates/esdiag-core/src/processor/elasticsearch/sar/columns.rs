// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Source-aligned field naming (ADR-0013) and unit normalization for `sar`
//! columns. Every column gets a name, including ones this catalog does not list.

use std::sync::Arc;

/// How a column's text becomes a JSON value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueKind {
    /// A float; `%` columns become ratios.
    Float { percent: bool, scale: f64 },
    /// An integer, such as a byte or table count, multiplied by `scale`.
    Integer { scale: i64 },
}

/// A column's resolved field name, relative to its section prefix, and value kind.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnSpec {
    pub name: Arc<str>,
    pub kind: ValueKind,
}

/// A per-section rename: the source column, its field name, and value kind.
pub type ColumnOverride = (&'static str, &'static str, ValueKind);

pub const FLOAT: ValueKind = ValueKind::Float {
    percent: false,
    scale: 1.0,
};
pub const PCT: ValueKind = ValueKind::Float {
    percent: true,
    scale: 1.0,
};
pub const KB_RATE: ValueKind = ValueKind::Float {
    percent: false,
    scale: 1024.0,
};
pub const SECTOR_RATE: ValueKind = ValueKind::Float {
    percent: false,
    scale: 512.0,
};
pub const KB: ValueKind = ValueKind::Integer { scale: 1024 };
pub const SECTORS: ValueKind = ValueKind::Integer { scale: 512 };
pub const COUNT: ValueKind = ValueKind::Integer { scale: 1 };

/// Columns holding whole counts rather than rates or measurements.
const COUNT_COLUMNS: &[&str] = &[
    "totsck",
    "tcpsck",
    "udpsck",
    "rawsck",
    "ip-frag",
    "tcp-tw",
    "tcp6sck",
    "udp6sck",
    "raw6sck",
    "ip6-frag",
    "dentunusd",
    "file-nr",
    "inode-nr",
    "pty-nr",
    "runq-sz",
    "plist-sz",
    "blocked",
    "maxpower",
];

/// Columns reporting a latency in milliseconds.
const LATENCY_COLUMNS: &[&str] = &["await", "r_await", "w_await", "d_await", "f_await", "svctm"];

/// Field-name suffixes and fragments the shared `esdiag@mappings` dynamic
/// templates disable or re-type.
const RESERVED_SUFFIXES: &[&str] = &[
    "_time",
    "time_string",
    "size",
    "_memory",
    "histogram",
    "timestamp",
    "date_millis",
];

/// Resolve a column through the section's overrides, then the generic rules.
pub fn column_spec(column: &str, overrides: &[ColumnOverride]) -> ColumnSpec {
    if let Some((_, name, kind)) = overrides.iter().find(|(source, _, _)| *source == column) {
        return ColumnSpec {
            name: Arc::from(*name),
            kind: *kind,
        };
    }
    let (name, kind) = generic(column);
    ColumnSpec {
        name: Arc::from(guard_reserved(name)),
        kind,
    }
}

fn generic(column: &str) -> (String, ValueKind) {
    if let Some(base) = column.strip_prefix('%') {
        return (
            format!("{}.pct", normalize(base)),
            ValueKind::Float {
                percent: true,
                scale: 1.0,
            },
        );
    }
    if let Some(base) = column
        .strip_prefix("kb")
        .filter(|base| !base.is_empty() && !base.contains('/'))
    {
        return (format!("{}.bytes", normalize(base)), KB);
    }
    if let Some(base) = column
        .strip_prefix("MB")
        .filter(|base| !base.is_empty() && !base.contains('/'))
    {
        return (
            format!("{}.bytes", normalize(base)),
            ValueKind::Integer { scale: 1_048_576 },
        );
    }
    if let Some(base) = column.strip_suffix("kB/s").filter(|base| !base.is_empty()) {
        return (format!("{}.bytes_per_sec", normalize(base)), KB_RATE);
    }
    if let Some(base) = column.strip_suffix("/s").filter(|base| !base.is_empty()) {
        return (format!("{}_per_sec", normalize(base)), FLOAT);
    }
    if LATENCY_COLUMNS.contains(&column) {
        return (format!("{}_ms", normalize(column)), FLOAT);
    }
    let kind = if COUNT_COLUMNS.contains(&column) { COUNT } else { FLOAT };
    (normalize(column), kind)
}

/// Lowercase, `-` to `_`, and anything outside `[a-z0-9_]` to `_`.
fn normalize(name: &str) -> String {
    name.chars()
        .map(|character| match character.to_ascii_lowercase() {
            lower @ ('a'..='z' | '0'..='9' | '_') => lower,
            _ => '_',
        })
        .collect()
}

/// Append `_value` to each dotted segment a reserved dynamic template would
/// capture; templates match parent objects too, which would disable every
/// field beneath them. The `*unsafe*` pattern matches anywhere, so that
/// fragment is split instead.
pub fn guard_reserved(name: String) -> String {
    let name = if name.contains("unsafe") {
        name.replace("unsafe", "un_safe")
    } else {
        name
    };
    if !is_reserved(&name) {
        return name;
    }
    name.split('.')
        .map(|segment| {
            if is_reserved_segment(segment) {
                format!("{segment}_value")
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

pub fn is_reserved(name: &str) -> bool {
    name.split('.').any(is_reserved_segment)
}

fn is_reserved_segment(segment: &str) -> bool {
    segment.contains("unsafe") || RESERVED_SUFFIXES.iter().any(|suffix| segment.ends_with(suffix))
}

/// A parsed cell value.
#[derive(Debug, Clone, PartialEq)]
pub enum SarValue {
    Float(f64),
    Integer(i64),
    Text(String),
}

impl serde::Serialize for SarValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            SarValue::Float(value) => serializer.serialize_f64(*value),
            SarValue::Integer(value) => serializer.serialize_i64(*value),
            SarValue::Text(value) => serializer.serialize_str(value),
        }
    }
}

impl ValueKind {
    /// Parse a cell; non-numeric text in a metric column yields `None`.
    pub fn parse(self, text: &str) -> Option<SarValue> {
        let text = localized_decimal(text)?;
        match self {
            ValueKind::Float { percent: true, .. } => {
                // Shifting the exponent keeps decimal input exact: `6.10` -> `0.061`.
                let value: f64 = format!("{text}e-2").parse().ok()?;
                value.is_finite().then_some(SarValue::Float(value))
            }
            ValueKind::Float { scale, .. } => {
                let value: f64 = text.parse().ok()?;
                let value = value * scale;
                value.is_finite().then_some(SarValue::Float(value))
            }
            ValueKind::Integer { scale } => match text.parse::<i64>() {
                Ok(value) => value.checked_mul(scale).map(SarValue::Integer),
                Err(_) => {
                    let value: f64 = text.parse().ok()?;
                    let value = (value * scale as f64).round();
                    value.is_finite().then_some(SarValue::Integer(value as i64))
                }
            },
        }
    }
}

/// Accept `5,71` from comma-decimal locales.
fn localized_decimal(text: &str) -> Option<std::borrow::Cow<'_, str>> {
    if text.is_empty() {
        return None;
    }
    if text.contains(',') && !text.contains('.') {
        Some(std::borrow::Cow::Owned(text.replacen(',', ".", 1)))
    } else {
        Some(std::borrow::Cow::Borrowed(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(column: &str) -> String {
        column_spec(column, &[]).name.to_string()
    }

    fn value(column: &str, text: &str) -> Option<SarValue> {
        column_spec(column, &[]).kind.parse(text)
    }

    #[test]
    fn generic_rules_name_every_unit() {
        assert_eq!(named("%usr"), "usr.pct");
        assert_eq!(named("%scpu-10"), "scpu_10.pct");
        assert_eq!(named("kbmemfree"), "memfree.bytes");
        assert_eq!(named("MBfsfree"), "fsfree.bytes");
        assert_eq!(named("rxkB/s"), "rx.bytes_per_sec");
        assert_eq!(named("zq-cnt/s"), "zq_cnt_per_sec");
        assert_eq!(named("ldavg-1"), "ldavg_1");
        assert_eq!(named("await"), "await_ms");
        assert_eq!(named("runq-sz"), "runq_sz");
    }

    #[test]
    fn values_normalize_units() {
        assert_eq!(value("%usr", "6.10"), Some(SarValue::Float(0.061)));
        assert_eq!(value("%vmeff", "141.31"), Some(SarValue::Float(1.4131)));
        assert_eq!(value("kbmemfree", "5523262"), Some(SarValue::Integer(5_655_820_288)));
        assert_eq!(value("MBfsfree", "2"), Some(SarValue::Integer(2_097_152)));
        assert_eq!(value("rxkB/s", "2.00"), Some(SarValue::Float(2048.0)));
        assert_eq!(value("totsck", "1342"), Some(SarValue::Integer(1342)));
        assert_eq!(value("%usr", "5,71"), Some(SarValue::Float(0.0571)));
        assert_eq!(SECTOR_RATE.parse("8.00"), Some(SarValue::Float(4096.0)));
    }

    #[test]
    fn non_numeric_metric_cells_are_omitted() {
        assert_eq!(value("%usr", "N/A"), None);
        assert_eq!(value("totsck", "-"), None);
        assert_eq!(value("tps", ""), None);
    }

    #[test]
    fn reserved_names_get_a_value_suffix() {
        assert_eq!(named("blk_time"), "blk_time_value");
        assert_eq!(named("pagesize"), "pagesize_value");
        assert_eq!(named("%unsafe"), "un_safe.pct");
        assert_eq!(named("%_time"), "_time_value.pct");
        assert_eq!(named("kb_time"), "_time_value.bytes");
        assert!(is_reserved("_time.pct"), "reserved parent segment");
        assert!(!is_reserved("aqu_sz"));
    }

    #[test]
    fn fuzzed_column_names_never_produce_reserved_names() {
        let stems = [
            "time",
            "_time",
            "size",
            "sz",
            "memory",
            "_memory",
            "histogram",
            "timestamp",
            "date_millis",
            "unsafe",
        ];
        let shapes = ["{}", "%{}", "kb{}", "MB{}", "{}kB/s", "{}/s", "x-{}", "{}-x", "X{}"];
        for stem in stems {
            for shape in shapes {
                let column = shape.replace("{}", stem);
                let name = named(&column);
                assert!(!is_reserved(&name), "{column} -> {name}");
            }
        }
    }
}
