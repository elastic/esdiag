// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    super::{DataProcessor, ElasticsearchMetadata, Lookups, Metadata},
    SearchableSnapshotsStats,
};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
impl DataProcessor<Lookups, ElasticsearchMetadata> for SearchableSnapshotsStats {
    fn generate_docs(
        self,
        _lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> (String, Vec<Value>) {
        let data_stream = "metrics-searchable_snapshot-esdiag".to_string();
        let searchable_snapshots_stats_metadata =
            metadata.for_data_stream(&data_stream).as_meta_doc();

        let mut indices: Vec<_> = self.indices.into_par_iter().collect();

        let searchable_snapshot_stats: Vec<Value> = indices
            .par_drain(..)
            .flat_map(|(index_name, mut index_stats)| {
                index_stats
                    .total
                    .par_drain(..)
                    .map(|index_stats| {
                        serde_json::to_value(SearchableSnapshotStatsDoc {
                            metadata: searchable_snapshots_stats_metadata.clone(),
                            index: IndexName {
                                name: index_name.clone(),
                            },
                            searchable_snapshot: serde_json::to_value(index_stats)
                                .expect("Failed to serialize searchable snapshot stats"),
                        })
                        .unwrap_or_default()
                    })
                    .collect::<Vec<Value>>()
            })
            .collect();

        log::debug!(
            "searchable_snapshot_stats docs: {}",
            searchable_snapshot_stats.len()
        );

        (data_stream, searchable_snapshot_stats)
    }
}

// Serializing data structures

#[derive(Clone, Serialize)]
pub struct SearchableSnapshotStatsDoc {
    #[serde(flatten)]
    metadata: Value,
    index: IndexName,
    searchable_snapshot: Value,
}

#[derive(Clone, Serialize)]
pub struct IndexName {
    pub name: String,
}
