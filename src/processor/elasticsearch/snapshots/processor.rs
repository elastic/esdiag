// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, ElasticsearchMetadata, Lookups, Metadata, ProcessorSummary};
use super::{SnapshotRepositories, Snapshots};
use crate::exporter::Exporter;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;

impl DocumentExporter<Lookups, ElasticsearchMetadata> for SnapshotRepositories {
    async fn documents_export(
        self,
        exporter: &Exporter,
        _lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        log::debug!("processing snapshot repositories");
        let data_stream = "settings-snapshot_repositories-esdiag".to_string();
        let metadata = metadata.for_data_stream(&data_stream).as_meta_doc();

        let mut repos: Vec<(String, Value)> = self.into_par_iter().collect();

        let repos: Vec<Value> = repos
            .par_drain(..)
            .filter_map(|(name, config)| {
                serde_json::to_value(SnapshotRepositoryDoc {
                    repository: SnapshotRepositoryDetails { name, config },
                    metadata: metadata.clone(),
                })
                .ok()
            })
            .collect();

        log::debug!("snapshot repositories docs: {}", repos.len());
        let mut summary = ProcessorSummary::new(data_stream.clone());
        match exporter.send(data_stream, repos).await {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => log::error!("Failed to send snapshot repositories: {}", err),
        }
        summary
    }
}

impl DocumentExporter<Lookups, ElasticsearchMetadata> for Snapshots {
    async fn documents_export(
        self,
        exporter: &Exporter,
        _lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        log::debug!("processing snapshots");
        let data_stream = "metadata-snapshots-esdiag".to_string();
        let metadata = metadata.for_data_stream(&data_stream).as_meta_doc();

        let snapshots: Vec<Value> = self.snapshots
            .into_par_iter()
            .filter_map(|snapshot| {
                serde_json::to_value(SnapshotDoc {
                    snapshot,
                    metadata: metadata.clone(),
                })
                .ok()
            })
            .collect();

        log::debug!("snapshots docs: {}", snapshots.len());
        let mut summary = ProcessorSummary::new(data_stream.clone());
        match exporter.send(data_stream, snapshots).await {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => log::error!("Failed to send snapshots: {}", err),
        }
        summary
    }
}

#[derive(Serialize)]
struct SnapshotRepositoryDoc {
    repository: SnapshotRepositoryDetails,
    #[serde(flatten)]
    metadata: Value,
}

#[derive(Serialize)]
struct SnapshotRepositoryDetails {
    name: String,
    #[serde(flatten)]
    config: Value,
}

#[derive(Serialize)]
struct SnapshotDoc {
    snapshot: Value,
    #[serde(flatten)]
    metadata: Value,
}
