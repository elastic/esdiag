use super::metadata::Metadata;
use json_patch::merge;
use serde::{self, Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
struct Total {
    total: Vec<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Indices {
    // Ignores _shards and total from root API response
    indices: HashMap<String, Value>,
}

pub fn enrich(metadata: &Metadata, data: String) -> Vec<Value> {
    let mut searchable_snapshot_stats = Vec::<Value>::new();
    let indices: Indices = match serde_json::from_str(&data) {
        Ok(data) => data,
        Err(e) => {
            log::error!("Failed to deserialize searchable_snapshots_stats: {}", e);
            return Vec::<Value>::new();
        }
    };

    log::debug!(
        "searchable_snapshot_stats indices: {}",
        indices.indices.len()
    );

    let metadata_patch = json!({
        "@timestamp": metadata.diagnostic.collection_date,
        "cluster": metadata.cluster,
        "diagnostic": metadata.diagnostic,
        "data_stream": {
            "dataset": "searchable_snapshot",
            "namespace": "esdiag",
            "type": "metrics",
        },
    });

    for (index, mut index_stats) in indices.indices.into_iter() {
        let total = index_stats["total"].take();
        let stats: Vec<Value> = match total {
            Value::Array(total) => total,
            _ => {
                log::warn!("No searchable_snapshot_stats for {index}");
                continue;
            }
        };

        let mut docs: Vec<Value> = stats
            .iter()
            .map(|index_stats| {
                let mut doc = json!({
                    "index": {
                        "name": index,
                    },
                    "searchable_snapshot": index_stats,
                });
                merge(&mut doc, &metadata_patch);
                doc
            })
            .collect();
        searchable_snapshot_stats.append(&mut docs);
    }

    log::debug!(
        "searchable_snapshot_stats docs: {}",
        searchable_snapshot_stats.len()
    );
    searchable_snapshot_stats
}
