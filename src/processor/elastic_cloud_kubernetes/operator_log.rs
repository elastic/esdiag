// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! `logs-eck.operator-esdiag`: operator pod logs, streamed line by line.

use super::{
    DocumentExporter, EckMetadata, ProcessorSummary,
    bundle::{EckLookups, OPERATOR_LOG_STREAM, is_missing_source_error},
    fields::{EckRef, KubernetesRef, Named},
    logs::{OperatorLogLine, parse_operator_line},
};
use crate::{exporter::Exporter, receiver::Receiver};
use futures::StreamExt;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

const BATCH_SIZE: usize = 5_000;

/// The operator pods whose `pod/<pod>/logs.txt` is read. No other pod log is
/// opened.
pub struct OperatorLogs {
    pub receiver: Arc<Receiver>,
    /// (namespace, pod, log path) of each operator pod.
    pub pods: Vec<(String, String, String)>,
}

#[derive(Serialize)]
struct OperatorLogDoc {
    #[serde(flatten)]
    line: OperatorLogLine,
    kubernetes: KubernetesRef,
    eck: OperatorLogEck,
}

#[derive(Serialize)]
struct OperatorLogEck {
    #[serde(flatten)]
    owner: EckRef,
    operator: OperatorPod,
}

#[derive(Serialize)]
struct OperatorPod {
    pod: Named,
    namespace: String,
}

fn log_doc(line: OperatorLogLine, operator_namespace: &str, operator_pod: &str) -> OperatorLogDoc {
    let owner = line
        .resource
        .as_ref()
        .map(|resource| EckRef::resource(&resource.kind, &resource.name))
        .unwrap_or_default();
    let kubernetes = line
        .namespace
        .as_deref()
        .map(KubernetesRef::namespace)
        .unwrap_or_default();
    OperatorLogDoc {
        line,
        kubernetes,
        eck: OperatorLogEck {
            owner,
            operator: OperatorPod {
                pod: Named::from(operator_pod),
                namespace: operator_namespace.to_string(),
            },
        },
    }
}

impl DocumentExporter<EckLookups, EckMetadata> for OperatorLogs {
    async fn documents_export(self, exporter: &Exporter, _: &EckLookups, metadata: &EckMetadata) -> ProcessorSummary {
        let mut summary = ProcessorSummary::new(OPERATOR_LOG_STREAM.to_string());
        let mut found = false;
        let mut read_failed = false;
        for (namespace, pod, path) in &self.pods {
            let mut lines = match self.receiver.read_bundle_lines(path).await {
                Ok(lines) => lines,
                Err(err) if is_missing_source_error(&err) => {
                    tracing::debug!("No operator log at {}", path);
                    continue;
                }
                Err(err) => {
                    summary = summary.with_error(format!("{path}: {err:#}"));
                    read_failed = true;
                    continue;
                }
            };
            found = true;

            let mut batch: Vec<Value> = Vec::with_capacity(BATCH_SIZE);
            while let Some(line) = lines.next().await {
                let line = match line {
                    Ok(line) => line,
                    Err(err) => {
                        summary = summary.with_error(format!("{path}: {err:#}"));
                        break;
                    }
                };
                if line.trim().is_empty() {
                    continue;
                }
                match parse_operator_line(&line) {
                    Some(parsed) => batch.push(metadata.document(OPERATOR_LOG_STREAM, log_doc(parsed, namespace, pod))),
                    None => summary.skipped_records = summary.skipped_records.saturating_add(1),
                }
                if batch.len() >= BATCH_SIZE {
                    send_batch(exporter, &mut summary, std::mem::take(&mut batch)).await;
                }
            }
            send_batch(exporter, &mut summary, batch).await;
        }

        verdict(summary, found, read_failed)
    }
}

/// Parsed when any operator log was read. When none was, the stream is
/// missing unless a log existed but could not be opened: absent logs, or no
/// operator pods at all, leave a partial bundle's outcome unchanged.
fn verdict(summary: ProcessorSummary, found: bool, read_failed: bool) -> ProcessorSummary {
    if found {
        summary.was_parsed()
    } else if read_failed {
        summary
    } else {
        ProcessorSummary::missing(OPERATOR_LOG_STREAM.to_string())
    }
}

async fn send_batch(exporter: &Exporter, summary: &mut ProcessorSummary, docs: Vec<Value>) {
    if docs.is_empty() {
        return;
    }
    match exporter.send(OPERATOR_LOG_STREAM.to_string(), docs).await {
        Ok(batch) => summary.add_batch(batch),
        Err(err) => tracing::error!("Failed to send ECK operator logs: {}", err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_doc_nests_resource_and_operator_pod_under_eck() {
        let line = parse_operator_line(
            r#"{"@timestamp":"2026-01-15T09:55:02.350Z","namespace":"demo","es_name":"quickstart","took":2.25}"#,
        )
        .unwrap();
        let doc = serde_json::to_value(log_doc(line, "elastic-system", "elastic-operator-0")).unwrap();
        assert_eq!(doc["eck"]["resource"]["kind"], "Elasticsearch");
        assert_eq!(doc["eck"]["resource"]["name"], "quickstart");
        assert_eq!(doc["eck"]["operator"]["pod"]["name"], "elastic-operator-0");
        assert_eq!(doc["kubernetes"]["namespace"], "demo");
        assert_eq!(doc["took"], 2.25);
        assert!(doc.get("resource").is_none());
    }

    #[test]
    fn absent_operator_logs_are_a_missing_stream() {
        let summary = || ProcessorSummary::new(OPERATOR_LOG_STREAM.to_string());
        let absent = verdict(summary(), false, false);
        assert!(absent.source.missing && !absent.source.parsed);

        let unreadable = verdict(summary().with_error("logs.txt: permission denied"), false, true);
        assert!(!unreadable.source.missing && !unreadable.source.parsed);

        let read = verdict(summary(), true, true);
        assert!(read.source.parsed);
    }
}
