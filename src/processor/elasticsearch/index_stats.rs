use super::metadata::Metadata;
use json_patch::merge;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{from_value, json, Map, Value};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
struct IndexStats {
    uuid: String,
    health: Option<String>,
    primaries: Value,
    total: Value,
    shards: Map<String, Value>,
}

fn divide_values(base: &Value, divisor: &Value) -> i64 {
    let base = match base.as_i64() {
        Some(start) => start,
        None => return 0,
    };
    let divisor = match divisor.as_i64() {
        Some(end) => end,
        None => return 0,
    };
    base / divisor
}

pub async fn enrich(metadata: &Metadata, mut data: Value) -> Vec<Value> {
    let mut indices: HashMap<String, IndexStats> =
        match from_value(data.get_mut("indices").unwrap().take()) {
            Ok(data) => data,
            Err(e) => {
                log::warn!("Failed to deserialize index_stats: {}", e);
                return Vec::<Value>::new();
            }
        };
    log::debug!("index_stats indices: {}", indices.len());

    let data_stream_shard_patch = json!({
        "@timestamp": metadata.diagnostic.collection_date,
        "cluster": metadata.cluster,
        "diagnostic": metadata.diagnostic,
        "data_stream": {
            "dataset": "shard",
            "namespace": "esdiag",
            "type": "metrics",
        }
    });

    let data_stream_index_patch = json!({
        "@timestamp": metadata.diagnostic.collection_date,
        "cluster": metadata.cluster,
        "diagnostic": metadata.diagnostic,
        "data_stream": {
            "dataset": "index",
            "namespace": "esdiag",
            "type": "metrics",
        },
        "index": {
            "shards": null
        }
    });

    let indices_stats: Vec<Value> = indices
        .par_iter_mut()
        .flat_map(|(index, ref mut index_stats)| {
            let shard_stats: Vec<_> = index_stats.shards.clone().into_iter().collect();
            index_stats.shards.clear();
            let data_stream = metadata
                .data_stream_lookup
                .by_index(index.as_str())
                .unwrap_or(Value::Null);
            let alias = metadata
                .alias_lookup
                .by_index(index.as_str())
                .unwrap_or(Value::Null);
            let mut docs: Vec<_> = shard_stats
                .par_iter()
                .flat_map(|(shard_id, shard_stats)| {
                    let mut shard_doc = json!({
                        "index": {
                            "alias": alias,
                            "name": index,
                            "uuid": index_stats.uuid,
                            "data_stream": data_stream,
                        },
                    });

                    merge(&mut shard_doc, &data_stream_shard_patch);
                    merge(&mut shard_doc, &json!({"shard": { "number": shard_id, }}));
                    let shard_docs: Vec<Value> = shard_stats
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|shard_stats| {
                            let mut doc = json!({
                                "shard":shard_stats,
                                "node": metadata.node_lookup.by_id(
                                    shard_stats["routing"]["node"].as_str().unwrap_or("")
                                ).unwrap_or(Value::Null),
                            });
                            merge(&mut doc, &shard_doc);
                            doc
                        })
                        .collect();
                    shard_docs
                })
                .collect();

            let indexing_patch = json!({
                "index": {
                    "primaries": {
                        "indexing": {
                            "index_time_per_shard_in_millis": divide_values(
                                &index_stats.primaries["indexing"]["index_time_in_millis"],
                                &index_stats.primaries["shard_stats"]["total_count"],
                            ),
                        }
                    },
                    "total": {
                        "indexing": {
                            "index_time_per_shard_in_millis": divide_values(
                                &index_stats.total["indexing"]["index_time_in_millis"],
                                &index_stats.total["shard_stats"]["total_count"],
                            )
                        }
                    }
                },
            });

            let mut doc = json!({"index": index_stats});
            merge(&mut doc, &data_stream_index_patch);
            merge(&mut doc, &indexing_patch);
            merge(
                &mut doc,
                &json!({
                    "index": {
                        "alias": alias,
                        "data_stream": data_stream,
                        "name": index,
                    }
                }),
            );
            docs.insert(0, doc);
            docs
        })
        .collect();

    log::debug!("index_stats docs: {}", indices_stats.len());
    indices_stats
}
