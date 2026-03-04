// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, ElasticsearchMetadata, Lookups, ProcessorSummary};
use crate::processor::StreamingDocumentExporter;
use super::{RepositoryConfig, Snapshot, SnapshotRepositories, Snapshots, extract_snapshot_date};
use crate::exporter::Exporter;
use eyre::Report;
use futures::stream::{BoxStream, StreamExt};
use serde::Serialize;
use serde_json::Value;
use serde_with::skip_serializing_none;
use tokio::sync::mpsc;

impl DocumentExporter<Lookups, ElasticsearchMetadata> for SnapshotRepositories {
    async fn documents_export(
        self,
        exporter: &Exporter,
        _lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        let data_stream = "settings-repository-esdiag".to_string();
        let metadata = metadata.for_data_stream(&data_stream).as_meta_doc();

        let repositories: Vec<RepositorySettingsDocument> = self
            .into_iter()
            .map(|(name, config)| RepositorySettingsDocument {
                repository: RepositoryDocument { name, config },
                metadata: metadata.clone(),
            })
            .collect();

        let mut summary = ProcessorSummary::new(data_stream.clone());
        match exporter.send(data_stream, repositories).await {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => log::error!("Failed to send repository settings: {}", err),
        }
        summary
    }
}

impl DocumentExporter<Lookups, ElasticsearchMetadata> for Snapshots {
    async fn documents_export(
        self,
        exporter: &Exporter,
        lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        let stream = futures::stream::iter(self.snapshots.into_iter().map(Ok));
        Self::documents_export_stream(Box::pin(stream), exporter, lookups, metadata).await
    }
}

impl StreamingDocumentExporter<Lookups, ElasticsearchMetadata> for Snapshots {
    async fn documents_export_stream(
        mut stream: BoxStream<'static, eyre::Result<Self::Item>>,
        exporter: &Exporter,
        _lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        let data_stream = "logs-snapshot-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let batch_size = 5000;
        const BUFFER_SIZE: usize = 5000;

        let (tx, rx) = mpsc::channel::<SnapshotLogDocument>(BUFFER_SIZE);
        let processor = tokio::spawn(
            exporter
                .clone()
                .document_channel::<SnapshotLogDocument>(rx, data_stream.clone(), batch_size),
        );

        while let Some(result) = stream.next().await {
            match result {
                Ok(snapshot) => {
                    if tx
                        .send(SnapshotLogDocument::from_snapshot(
                            snapshot,
                            metadata_doc.clone(),
                        ))
                        .await
                        .is_err()
                    {
                        log::warn!("Snapshot channel closed unexpectedly");
                        break;
                    }
                }
                Err(err) => {
                    log::warn!("Error reading from snapshot stream: {}", err);
                }
            }
        }

        drop(tx);
        let mut summary = ProcessorSummary::new(data_stream);
        summary.merge(processor.await.map_err(Report::new));
        summary
    }
}

#[derive(Serialize)]
struct RepositorySettingsDocument {
    repository: RepositoryDocument,
    #[serde(flatten)]
    metadata: Value,
}

#[derive(Serialize)]
struct RepositoryDocument {
    name: String,
    #[serde(flatten)]
    config: RepositoryConfig,
}

#[derive(Serialize)]
struct SnapshotLogDocument {
    snapshot: SnapshotDocument,
    #[serde(flatten)]
    metadata: Value,
}

impl SnapshotLogDocument {
    fn from_snapshot(snapshot: Snapshot, metadata: Value) -> Self {
        Self {
            snapshot: SnapshotDocument::from(snapshot),
            metadata,
        }
    }
}

#[skip_serializing_none]
#[derive(Serialize)]
struct SnapshotDocument {
    name: String,
    repository: Option<String>,
    state: Option<String>,
    indices: Option<Vec<String>>,
    data_streams: Option<Vec<String>>,
    date: Option<String>,
    start_time: Option<String>,
    start_time_in_millis: Option<u64>,
    end_time: Option<String>,
    end_time_in_millis: Option<u64>,
    duration_in_millis: Option<u64>,
}

impl From<Snapshot> for SnapshotDocument {
    fn from(snapshot: Snapshot) -> Self {
        Self {
            date: extract_snapshot_date(&snapshot.snapshot),
            name: snapshot.snapshot,
            repository: snapshot.repository,
            state: snapshot.state,
            indices: snapshot.indices,
            data_streams: snapshot.data_streams,
            start_time: snapshot.start_time,
            start_time_in_millis: snapshot.start_time_in_millis,
            end_time: snapshot.end_time,
            end_time_in_millis: snapshot.end_time_in_millis,
            duration_in_millis: snapshot.duration_in_millis,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn snapshot_document_contains_date_when_name_has_token() {
        let snapshot = Snapshot {
            snapshot: "daily-2026.03.01".to_string(),
            repository: Some("repo-a".to_string()),
            state: Some("SUCCESS".to_string()),
            indices: Some(vec!["logs-1".to_string()]),
            data_streams: Some(vec!["logs-app".to_string()]),
            start_time: Some("2026-03-01T01:00:00.000Z".to_string()),
            start_time_in_millis: Some(1709254800000),
            end_time: Some("2026-03-01T01:10:00.000Z".to_string()),
            end_time_in_millis: Some(1709255400000),
            duration_in_millis: Some(600000),
        };

        let value = serde_json::to_value(SnapshotLogDocument::from_snapshot(snapshot, json!({})))
            .expect("serialize snapshot doc");
        assert_eq!(value["snapshot"]["date"], "2026-03-01");
    }

    #[test]
    fn repository_document_contains_core_fields() {
        let mut repos = SnapshotRepositories::new();
        repos.insert(
            "repo-a".to_string(),
            RepositoryConfig {
                repository_type: Some("s3".to_string()),
                settings: Some(json!({"bucket":"backup"})),
                extra: std::collections::HashMap::new(),
            },
        );
        let metadata = json!({});
        let docs: Vec<RepositorySettingsDocument> = repos
            .into_iter()
            .map(|(name, config)| RepositorySettingsDocument {
                repository: RepositoryDocument { name, config },
                metadata: metadata.clone(),
            })
            .collect();
        let value = serde_json::to_value(&docs[0]).expect("serialize repository doc");
        assert_eq!(value["repository"]["name"], "repo-a");
        assert_eq!(value["repository"]["type"], "s3");
        assert_eq!(value["repository"]["settings"]["bucket"], "backup");
    }
}
