// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::columns::SarValue;
use super::parser::{Fields, SarRecord, Target};
use super::section::Metricset;
use crate::processor::elasticsearch::metadata::MetadataValue;
use chrono::NaiveDateTime;
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Every SAR document's `sar.timezone`: sample times carry no zone.
pub const TIMEZONE: &str = "assumed_utc";

/// The parts of one SAR document, before its envelope is attached.
#[derive(Debug, Clone, PartialEq)]
pub struct SarDocParts {
    pub timestamp: NaiveDateTime,
    pub metricset: Metricset,
    pub keys: Vec<(&'static str, SarValue)>,
    pub fields: Fields,
}

/// One document in `metrics-host.sar-esdiag`.
pub struct SarDoc {
    pub metadata: MetadataValue,
    /// Host identity and node correlation, shared by every document.
    pub host: Arc<Map<String, Value>>,
    pub parts: SarDocParts,
}

impl Serialize for SarDoc {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        if let Some(metadata) = self.metadata.0.as_object() {
            for (key, value) in metadata {
                map.serialize_entry(key, value)?;
            }
        }
        for (key, value) in self.host.iter() {
            map.serialize_entry(key, value)?;
        }
        map.serialize_entry("sar.metricset", self.parts.metricset.as_str())?;
        map.serialize_entry("sar.timezone", TIMEZONE)?;
        for (key, value) in &self.parts.keys {
            map.serialize_entry(key, value)?;
        }
        for (key, value) in &self.parts.fields {
            map.serialize_entry(key.as_ref(), value)?;
        }
        map.end()
    }
}

/// Assembles records into documents. Several sections contribute to the
/// same `summary`, `cpu`, and `network` documents, so those are buffered
/// until the file ends; every other record is a complete document.
#[derive(Default)]
pub struct SarAssembler {
    summary: BTreeMap<NaiveDateTime, Fields>,
    cpu: BTreeMap<(NaiveDateTime, CpuId), Fields>,
    network: BTreeMap<(NaiveDateTime, String), Fields>,
}

/// CPU identifiers order numerically, then as text.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum CpuId {
    Number(u32),
    Name(String),
}

impl CpuId {
    fn new(id: String) -> Self {
        match id.parse() {
            Ok(number) => CpuId::Number(number),
            Err(_) => CpuId::Name(id),
        }
    }

    fn into_text(self) -> String {
        match self {
            CpuId::Number(number) => number.to_string(),
            CpuId::Name(name) => name,
        }
    }
}

fn merge(into: &mut Fields, fields: Fields) {
    for (key, value) in fields {
        match into.iter_mut().find(|(existing, _)| *existing == key) {
            Some((_, existing)) => *existing = value,
            None => into.push((key, value)),
        }
    }
}

impl SarAssembler {
    /// Buffer a mergeable record, or return a single-section record as a document.
    pub fn push(&mut self, record: SarRecord) -> Option<SarDocParts> {
        let SarRecord {
            timestamp,
            target,
            fields,
        } = record;
        match target {
            Target::Summary => merge(self.summary.entry(timestamp).or_default(), fields),
            Target::Cpu(cpu) => merge(self.cpu.entry((timestamp, CpuId::new(cpu))).or_default(), fields),
            Target::Network(interface) => merge(self.network.entry((timestamp, interface)).or_default(), fields),
            Target::Entity { metricset, keys } => {
                return Some(SarDocParts {
                    timestamp,
                    metricset,
                    keys,
                    fields,
                });
            }
        }
        None
    }

    /// The buffered documents, in time order per metricset.
    pub fn drain(self) -> impl Iterator<Item = SarDocParts> {
        let summary = self.summary.into_iter().map(|(timestamp, fields)| SarDocParts {
            timestamp,
            metricset: Metricset::Summary,
            keys: Vec::new(),
            fields,
        });
        let cpu = self.cpu.into_iter().map(|((timestamp, cpu), fields)| SarDocParts {
            timestamp,
            metricset: Metricset::Cpu,
            keys: vec![("sar.cpu.id", SarValue::Text(cpu.into_text()))],
            fields,
        });
        let network = self
            .network
            .into_iter()
            .map(|((timestamp, interface), fields)| SarDocParts {
                timestamp,
                metricset: Metricset::Network,
                keys: vec![("sar.network.interface", SarValue::Text(interface))],
                fields,
            });
        summary.chain(cpu).chain(network)
    }
}

#[cfg(test)]
mod tests {
    use super::super::columns::is_reserved;
    use super::super::fixture::{self, CPUS, DEVICES, INTERFACES, MATRIX_INTERRUPTS};
    use super::super::parser::SarParser;
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    const SAMPLES: [&str; 3] = ["12:10:10 AM", "12:20:10 AM", "12:30:10 AM"];

    fn assemble(text: &str) -> Vec<SarDocParts> {
        let mut parser = SarParser::new(None);
        let mut assembler = SarAssembler::default();
        let mut records = Vec::new();
        let mut docs = Vec::new();
        for line in text.lines() {
            parser.push_line(line, &mut records);
            docs.extend(records.drain(..).filter_map(|record| assembler.push(record)));
        }
        let outcome = parser.finish();
        assert_eq!(outcome.problem, None);
        assert!(outcome.unknown_headers.is_empty(), "{:?}", outcome.unknown_headers);
        docs.extend(assembler.drain());
        docs
    }

    fn all_sections() -> Vec<SarDocParts> {
        assemble(&fixture::all_sections(&SAMPLES))
    }

    fn has(doc: &SarDocParts, path: &str) -> bool {
        doc.fields.iter().any(|(key, _)| key.as_ref() == path)
    }

    #[test]
    fn every_metricset_has_one_document_per_sample_and_entity() {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for doc in all_sections() {
            *counts.entry(doc.metricset.as_str()).or_default() += 1;
        }
        let samples = SAMPLES.len();
        let expected = BTreeMap::from([
            ("summary", samples),
            ("cpu", samples * CPUS),
            ("io", samples * DEVICES.len()),
            ("network", samples * INTERFACES.len()),
            ("filesystem", samples),
            // Two `INTR` rows plus the per-CPU matrix.
            ("interrupt", samples * (2 + CPUS * MATRIX_INTERRUPTS)),
            ("tty", samples * 2),
            ("fan", samples * 2),
            ("temp", samples * 2),
            ("in", samples * 2),
            ("usb", samples),
            ("fc_host", samples),
        ]);
        assert_eq!(counts, expected);
    }

    #[test]
    fn summary_merges_every_host_wide_section() {
        let docs = all_sections();
        let summary: Vec<&SarDocParts> = docs.iter().filter(|doc| doc.metricset == Metricset::Summary).collect();
        for doc in &summary {
            for path in [
                "sar.cpu.usr.pct",
                "sar.softnet.total_per_sec",
                "sar.cpu.mhz",
                "sar.memory.memfree.bytes",
                "sar.load.runq_sz",
                "sar.sockets.totsck",
                "sar.nfs.retrans_per_sec",
                "sar.nfsd.scall_per_sec",
                "sar.tcp.active_per_sec",
                "sar.etcp.retrans_per_sec",
                "sar.pressure.scpu_10.pct",
                "sar.interrupts.intr_per_sec",
                "sar.ip6.irec6_per_sec",
            ] {
                assert!(has(doc, path), "summary at {} lacks {path}", doc.timestamp);
            }
        }
    }

    #[test]
    fn per_cpu_and_per_interface_sections_merge() {
        let docs = all_sections();
        let cpu = docs
            .iter()
            .filter(|doc| doc.metricset == Metricset::Cpu)
            .find(|doc| doc.keys == [("sar.cpu.id", SarValue::Text("1".into()))])
            .expect("cpu 1");
        for path in [
            "sar.cpu.usr.pct",
            "sar.softnet.total_per_sec",
            "sar.cpu.mhz",
            "sar.cpu.wghmhz",
        ] {
            assert!(has(cpu, path), "cpu document lacks {path}");
        }
        let eth0 = docs
            .iter()
            .find(|doc| doc.keys == [("sar.network.interface", SarValue::Text("eth0".into()))])
            .expect("eth0");
        assert!(has(eth0, "sar.network.rx.packets_per_sec"));
        assert!(has(eth0, "sar.network.rx.errors_per_sec"));
    }

    #[test]
    fn entity_documents_carry_only_their_own_prefixes() {
        for doc in all_sections() {
            let prefixes: &[&str] = match doc.metricset {
                Metricset::Summary => continue,
                Metricset::Cpu => &["sar.cpu.", "sar.softnet."],
                Metricset::Io => &["sar.io."],
                Metricset::Network => &["sar.network."],
                Metricset::Filesystem => &["sar.filesystem."],
                Metricset::Interrupt => &["sar.interrupt."],
                Metricset::Tty => &["sar.tty."],
                Metricset::Fan => &["sar.fan."],
                Metricset::Temp => &["sar.temp."],
                Metricset::In => &["sar.in."],
                Metricset::Usb => &["sar.usb."],
                Metricset::FcHost => &["sar.fc_host."],
            };
            let keys = doc.keys.iter().map(|(key, _)| *key);
            let fields = doc.fields.iter().map(|(key, _)| key.as_ref());
            for path in keys.chain(fields) {
                assert!(
                    prefixes.iter().any(|prefix| path.starts_with(prefix)),
                    "{} document carries {path}",
                    doc.metricset.as_str()
                );
            }
        }
    }

    #[test]
    fn generated_fields_have_one_type_and_avoid_reserved_names() {
        let mut types: BTreeMap<String, &'static str> = BTreeMap::new();
        for doc in all_sections() {
            let keys = doc.keys.iter().map(|(key, value)| (key.to_string(), value));
            let fields = doc.fields.iter().map(|(key, value)| (key.to_string(), value));
            for (path, value) in keys.chain(fields) {
                let kind = match value {
                    SarValue::Float(_) => "float",
                    SarValue::Integer(_) => "long",
                    SarValue::Text(_) => "keyword",
                };
                assert!(!is_reserved(&path), "{path} matches a reserved dynamic template");
                let existing = types.entry(path.clone()).or_insert(kind);
                assert_eq!(*existing, kind, "{path} has two value types");
            }
        }
    }

    #[test]
    fn no_field_is_also_an_object_parent() {
        let paths: BTreeSet<String> = all_sections()
            .iter()
            .flat_map(|doc| {
                let keys = doc.keys.iter().map(|(key, _)| key.to_string());
                keys.chain(doc.fields.iter().map(|(key, _)| key.to_string()))
            })
            .collect();
        for path in &paths {
            let prefix = format!("{path}.");
            let children: Vec<&String> = paths.iter().filter(|other| other.starts_with(&prefix)).collect();
            assert!(children.is_empty(), "{path} is a value and the parent of {children:?}");
        }
    }

    #[test]
    fn idle_entities_are_kept() {
        let docs = assemble(&format!(
            "{}\n\n12:00:01 AM DEV tps %util\n12:10:10 AM sda 0.00 0.00\n12:20:10 AM sda 0.00 0.00\n\n12:00:01 AM IFACE rxpck/s txpck/s\n12:10:10 AM lo 0.00 0.00\n",
            fixture::HEADER
        ));
        assert_eq!(docs.iter().filter(|doc| doc.metricset == Metricset::Io).count(), 2);
        assert_eq!(docs.iter().filter(|doc| doc.metricset == Metricset::Network).count(), 1);
    }

    #[test]
    fn documents_serialize_flat_dotted_fields_after_the_envelopes() {
        let parts = SarDocParts {
            timestamp: NaiveDateTime::parse_from_str("2026-08-05 00:10:10", "%Y-%m-%d %H:%M:%S").unwrap(),
            metricset: Metricset::Io,
            keys: vec![("sar.io.device", SarValue::Text("sda".into()))],
            fields: vec![(Arc::from("sar.io.util.pct"), SarValue::Float(0.5))],
        };
        let doc = SarDoc {
            metadata: MetadataValue(Arc::new(serde_json::json!({"@timestamp": 1}))),
            host: Arc::new(
                serde_json::json!({"host": {"name": "node-01.example.com"}})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            parts,
        };
        let value = serde_json::to_value(&doc).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "@timestamp": 1,
                "host": {"name": "node-01.example.com"},
                "sar.metricset": "io",
                "sar.timezone": "assumed_utc",
                "sar.io.device": "sda",
                "sar.io.util.pct": 0.5
            })
        );
    }
}
