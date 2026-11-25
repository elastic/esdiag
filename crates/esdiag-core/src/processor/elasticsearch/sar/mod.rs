// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Host activity history from `syscalls/sar.txt` (`sar -A` text output),
//! exported as host-scoped documents to one data stream.

mod clean;
mod columns;
mod device;
mod docs;
mod header;
mod host;
mod parser;
mod section;
mod timestamp;

#[cfg(test)]
mod fixture;

pub use host::manifest_target_host;

use super::{ElasticsearchDiagnostic, ElasticsearchMetadata};
use crate::exporter::spawn_consumer;
use crate::processor::ProcessorSummary;
use crate::processor::elasticsearch::metadata::MetadataValue;
use crate::processor::syscall::{SyscallLines, catalog::option_key, syscall_source};
use crate::receiver::Receiver;
use chrono::{DateTime, NaiveDate, NaiveDateTime};
use docs::{SarAssembler, SarDoc, SarDocParts};
use futures::{StreamExt, future::BoxFuture};
use host::{CorrelationInput, NodeCorrelation, SocketListing, host_envelope, listening_socket};
use parser::{ParseProblem, SarParser};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;

/// The syscall catalog key this processor handles.
pub const KEY: &str = "sar";
pub const DATA_STREAM: &str = "metrics-host.sar-esdiag";
const BATCH_SIZE: usize = 5000;

/// The Elasticsearch syscall dispatch entry for `sar`.
pub fn process(diagnostic: Arc<ElasticsearchDiagnostic>, lines: SyscallLines) -> BoxFuture<'static, ProcessorSummary> {
    Box::pin(async move { process_sar(&diagnostic, lines).await })
}

fn collection_date(millis: u64) -> Option<NaiveDate> {
    DateTime::from_timestamp_millis(i64::try_from(millis).ok()?).map(|date| date.date_naive())
}

/// Concrete listening sockets from `ss`, or from `netstat` when `ss` is absent.
async fn listening_sockets(receiver: &Receiver) -> Vec<SocketAddr> {
    for (key, listing) in [("ss", SocketListing::Ss), ("netstat", SocketListing::Netstat)] {
        let Some(source) = syscall_source(key) else {
            continue;
        };
        match receiver.read_bundle_lines(&source.filename).await {
            Ok(lines) => {
                let mut sockets: Vec<SocketAddr> = lines
                    .filter_map(|line| async move { line.ok().and_then(|line| listening_socket(&line, listing)) })
                    .collect()
                    .await;
                sockets.sort();
                sockets.dedup();
                return sockets;
            }
            Err(err) => tracing::debug!("{} is unavailable for SAR correlation: {}", source.filename, err),
        }
    }
    Vec::new()
}

/// Attaches envelopes to finished documents and sends them to the exporter.
struct SarSender<'a> {
    metadata: &'a ElasticsearchMetadata,
    tx: mpsc::Sender<SarDoc>,
    host: Arc<Map<String, Value>>,
    /// One envelope per sample time, shared by every document at that time.
    envelopes: HashMap<NaiveDateTime, MetadataValue>,
    /// Why the exporter stopped accepting documents, once it has.
    closed: Option<String>,
}

impl SarSender<'_> {
    async fn send(&mut self, parts: SarDocParts) {
        if self.closed.is_some() {
            return;
        }
        let metadata = self
            .envelopes
            .entry(parts.timestamp)
            .or_insert_with(|| {
                let millis = u64::try_from(parts.timestamp.and_utc().timestamp_millis()).unwrap_or_default();
                self.metadata.for_data_stream_at(DATA_STREAM, millis)
            })
            .clone();
        let doc = SarDoc {
            metadata,
            host: self.host.clone(),
            parts,
        };
        if self.tx.send(doc).await.is_err() {
            self.closed = Some("the SAR exporter stopped accepting documents; later samples were not exported".into());
        }
    }
}

async fn process_sar(diagnostic: &ElasticsearchDiagnostic, mut lines: SyscallLines) -> ProcessorSummary {
    let option = option_key(KEY);
    let filename = syscall_source(KEY).map_or("syscalls/sar.txt", |source| source.filename.as_str());
    let listening = listening_sockets(&diagnostic.receiver).await;

    let (tx, rx) = mpsc::channel::<SarDoc>(BATCH_SIZE);
    let consumer = spawn_consumer((*diagnostic.exporter).clone().document_channel::<SarDoc>(
        rx,
        DATA_STREAM.to_string(),
        BATCH_SIZE,
    ));

    let mut parser = SarParser::new(collection_date(diagnostic.metadata.timestamp));
    let mut assembler = SarAssembler::default();
    let mut sender: Option<SarSender> = None;
    let mut records = Vec::new();
    let mut first_line = None;
    let mut line_number = 0usize;
    let mut read_error = None;

    while let Some(line) = lines.next().await {
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                read_error = Some(format!("{filename} could not be read past line {line_number}: {err:#}"));
                break;
            }
        };
        line_number += 1;
        if first_line.is_none() {
            first_line = Some(line.trim().chars().take(80).collect::<String>());
        }
        parser.push_line(&line, &mut records);
        if records.is_empty() {
            continue;
        }
        if sender.is_none()
            && let Some(header) = parser.header()
        {
            let correlation = NodeCorrelation::correlate(&CorrelationInput {
                hostname: &header.hostname,
                nodes: &diagnostic.lookups.node_endpoints,
                listening: &listening,
                manifest_host: diagnostic.manifest_host.as_deref(),
            });
            tracing::debug!(
                "SAR host correlated by {} with {} node(s)",
                correlation.method.as_str(),
                correlation.nodes.len()
            );
            sender = Some(SarSender {
                metadata: &diagnostic.metadata,
                tx: tx.clone(),
                host: Arc::new(host_envelope(header, &listening, &correlation)),
                envelopes: HashMap::new(),
                closed: None,
            });
        }
        let Some(sender) = sender.as_mut() else {
            records.clear();
            continue;
        };
        for record in records.drain(..) {
            if let Some(parts) = assembler.push(record) {
                sender.send(parts).await;
            }
        }
    }

    if let Some(sender) = sender.as_mut() {
        for parts in assembler.drain() {
            sender.send(parts).await;
        }
    }
    let closed = sender.and_then(|sender| sender.closed);
    drop(tx);

    let mut summary = match consumer.await {
        Ok(summary) => summary.rename(option.clone()).was_parsed(),
        Err(err) => ProcessorSummary::new(option.clone())
            .was_parsed()
            .with_error(format!("SAR export task failed: {err}")),
    };

    let outcome = parser.finish();
    match outcome.problem {
        Some(ParseProblem::NoHeader) => {
            summary = summary.with_warning(format!(
                "{filename} has no sar report header (a line starting with `Linux`); it begins: {}",
                first_line.unwrap_or_default()
            ));
        }
        Some(ParseProblem::NoData) => {
            summary = summary.with_warning(format!("{filename} has a sar report header but no sample rows"));
        }
        None => {}
    }
    for date in outcome.ambiguous_dates {
        summary = summary.with_warning(format!(
            "{filename} has a report date that reads as day/month or month/day; assumed {date}"
        ));
    }
    if let Some(closed) = closed {
        summary = summary.with_warning(closed);
    }
    for columns in outcome.unknown_headers {
        summary = summary.with_warning(format!(
            "{filename} has a section with unrecognized columns that was skipped: {columns}"
        ));
    }
    if let Some(read_error) = read_error {
        summary = summary.with_warning(read_error);
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Uri;
    use crate::exporter::Exporter;
    use crate::processor::{DiagnosticProcessor, EventSeverity};
    use futures::stream;

    #[tokio::test(flavor = "multi_thread")]
    async fn a_mid_file_read_error_keeps_earlier_documents_and_warns() {
        let root = tempfile::tempdir().expect("bundle dir");
        let archive = std::fs::File::open(format!(
            "{}/../../tests/archives/elasticsearch-api-diagnostics-9.3.3.zip",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("fixture archive");
        zip::ZipArchive::new(archive)
            .expect("zip")
            .extract(root.path())
            .expect("extract");
        let receiver = Arc::new(Receiver::try_from(Uri::Directory(root.path().to_path_buf())).expect("receiver"));
        let output = tempfile::tempdir().expect("output dir");
        let exporter = Arc::new(Exporter::try_from(Uri::Directory(output.path().to_path_buf())).expect("exporter"));
        let manifest = receiver.try_get_manifest().await.expect("manifest");
        let (diagnostic, _) = ElasticsearchDiagnostic::try_new(receiver, exporter, manifest, None)
            .await
            .expect("diagnostic");

        let lines: SyscallLines = stream::iter([
            Ok(fixture::HEADER.to_string()),
            Ok(String::new()),
            Ok("12:00:01 AM DEV tps".to_string()),
            Ok("12:10:10 AM sda 1.00".to_string()),
            Err(eyre::eyre!("device vanished")),
            Ok("12:20:10 AM sda 2.00".to_string()),
        ])
        .boxed();
        let summary = process(Arc::new(*diagnostic), lines).await;

        assert_eq!(summary.processor, "syscalls.sar");
        assert!(summary.source.parsed);
        assert_eq!(summary.docs, 1);
        let warnings: Vec<&str> = summary
            .events()
            .iter()
            .filter(|event| event.severity == EventSeverity::Warning)
            .map(|event| event.reason.as_str())
            .collect();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("past line 4") && warnings[0].contains("device vanished"));
    }
}
