use super::metadata::{DataStream, Metadata, MetadataDoc};
use rayon::prelude::*;
use serde::{self, Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn enrich(metadata: &Metadata, data: String) -> Vec<Value> {
    let data = match serde_json::from_str::<Indices>(&data) {
        Ok(data) => data,
        Err(e) => {
            log::error!("Failed to deserialize searchable_snapshots_stats: {}", e);
            return Vec::new();
        }
    };
    let indices: Vec<_> = data.indices.into_iter().collect();

    let searchable_snapshot_doc = SearchableSnapshotStatsDoc::new(
        metadata.as_doc.clone(),
        DataStream::from("metrics-searchable_snapshot-esdiag"),
    );

    let searchable_snapshot_stats: Vec<Value> = indices
        .par_iter()
        .flat_map(|(index, index_stats)| {
            index_stats
                .total
                .par_iter()
                .map(|index_stats| {
                    json!(searchable_snapshot_doc
                        .clone()
                        .with(index.clone(), index_stats.clone()))
                })
                .collect::<Vec<Value>>()
        })
        .collect();

    log::debug!(
        "searchable_snapshot_stats docs: {}",
        searchable_snapshot_stats.len()
    );

    searchable_snapshot_stats
}

// Serializing data structures

#[derive(Clone, Serialize)]
pub struct SearchableSnapshotStatsDoc {
    #[serde(flatten)]
    metadata: MetadataDoc,
    data_stream: DataStream,
    index: Option<IndexName>,
    searchable_snapshot: Value,
}

#[derive(Clone, Serialize)]
pub struct IndexName {
    pub name: String,
}

impl SearchableSnapshotStatsDoc {
    pub fn new(metadata: MetadataDoc, data_stream: DataStream) -> Self {
        SearchableSnapshotStatsDoc {
            data_stream,
            index: None,
            metadata,
            searchable_snapshot: Value::Null,
        }
    }
    pub fn with(mut self, index: String, searchable_snapshot: Value) -> Self {
        self.index = Some(IndexName {
            name: index.clone(),
        });
        self.searchable_snapshot = searchable_snapshot;
        self
    }
}

// Deserializing data structures

#[derive(Debug, Serialize, Deserialize)]
struct Total {
    total: Vec<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Indices {
    indices: HashMap<String, Total>,
}
