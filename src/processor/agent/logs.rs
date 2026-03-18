// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{AgentMetadata, Metadata};
use crate::{exporter::Exporter, receiver::Receiver};
use crate::processor::ProcessorSummary;
use eyre::Result;
use serde_json::Value;
use std::sync::Arc;

const DATA_STREAM: &str = "logs-elastic.agent-esdiag";

/// Export all log files, enriching each NDJSON line with agent metadata.
/// Log file paths are relative to the receiver root (e.g. "logs/data/elastic-agent-20260315.ndjson").
pub async fn export_logs(
    log_files: &[String],
    receiver: &Arc<Receiver>,
    exporter: &Exporter,
    metadata: &AgentMetadata,
) -> Result<ProcessorSummary> {
    let meta = metadata.for_data_stream(DATA_STREAM).as_meta_doc();
    let mut summary = ProcessorSummary::new(DATA_STREAM.to_string());

    for log_file in log_files {
        log::debug!("Processing log file: {}", log_file);
        let content = match receiver.read_file_string(log_file).await {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Failed to read log file {}: {}", log_file, e);
                continue;
            }
        };

        let mut batch: Vec<Value> = Vec::new();

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }

            let mut log_entry: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(e) => {
                    log::debug!("Skipping non-JSON line in {}: {}", log_file, e);
                    continue;
                }
            };

            // Enrich with metadata (agent.*, diagnostic.*, host.*, os.*, data_stream.*)
            // Preserve the original @timestamp from the log line.
            // Overwrite data_stream to ensure routing to esdiag, not the
            // agent's own monitoring data stream embedded in the log entry.
            if let Value::Object(ref mut map) = log_entry {
                let original_timestamp = map.get("@timestamp").cloned();
                if let Value::Object(meta_map) = &meta {
                    for (key, value) in meta_map {
                        if key == "@timestamp" {
                            continue; // preserve original timestamp
                        }
                        if key == "data_stream" {
                            map.insert(key.clone(), value.clone()); // overwrite
                        } else {
                            map.entry(key.clone()).or_insert(value.clone());
                        }
                    }
                }
                if let Some(ts) = original_timestamp {
                    map.insert("@timestamp".to_string(), ts);
                }
            }

            batch.push(log_entry);

            // Send in batches to avoid excessive memory usage
            if batch.len() >= 500 {
                match exporter.send(DATA_STREAM.to_string(), batch).await {
                    Ok(b) => summary.add_batch(b),
                    Err(err) => log::error!("Failed to send log batch: {}", err),
                }
                batch = Vec::new();
            }
        }

        // Send remaining batch
        if !batch.is_empty() {
            match exporter.send(DATA_STREAM.to_string(), batch).await {
                Ok(b) => summary.add_batch(b),
                Err(err) => log::error!("Failed to send log batch: {}", err),
            }
        }
    }

    Ok(summary)
}
