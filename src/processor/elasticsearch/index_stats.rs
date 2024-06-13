use super::metadata::Metadata;
use crate::processor::elasticsearch::lookup::index::IndexData;
use async_std::fs::write;
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

pub fn enrich(metadata: &Metadata, mut data: Value) -> Vec<Value> {
    let mut indices: HashMap<String, IndexStats> = match from_value(
        data.get_mut("indices")
            .expect("Failed to get indices")
            .take(),
    ) {
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
            let data_stream = metadata.lookup.data_stream.by_name(index.as_str());
            let alias = metadata.lookup.alias.by_name(index.as_str());
            let ilm = metadata.lookup.ilm.by_name(index);
            let index_data = metadata.lookup.index.by_name(index).unwrap_or(&IndexData {
                indexing_complete: None,
                creation_date: None,
            });
            let since_creation = match ilm {
                Some(ilm) => match ilm.index_creation_date_millis {
                    Some(date) => Some(metadata.diagnostic.collection_date - date),
                    None => match index_data.creation_date {
                        Some(date) => Some(metadata.diagnostic.collection_date - date),
                        None => None,
                    },
                },
                None => None,
            };
            let since_rollover = match ilm {
                Some(ilm) => match ilm.lifecycle_date_millis {
                    Some(date) => Some(metadata.diagnostic.collection_date - date),
                    None => None,
                },
                None => None,
            };
            let write_window_sec = match (since_creation, since_rollover) {
                (Some(creation), Some(rollover)) => (creation - rollover) / 1000,
                _ => 0,
            };

            let mut docs: Vec<_> = shard_stats
                .par_iter()
                .flat_map(|(shard_id, shard_stats)| {
                    let mut shard_doc = json!({
                        "index": {
                            "alias": alias,
                            "data_stream": data_stream,
                            "ilm": ilm,
                            "name": index,
                            "uuid": index_stats.uuid,
                            "since_creation": since_creation,
                            "since_rollover": since_rollover,
                            "indexing_complete": index_data.indexing_complete,
                            "creation_date": index_data.creation_date,
                            "write_window_sec": write_window_sec,
                        },
                    });

                    merge(&mut shard_doc, &data_stream_shard_patch);
                    merge(&mut shard_doc, &json!({"shard": { "number": shard_id, }}));
                    let shard_docs: Vec<Value> = shard_stats
                        .as_array()
                        .expect("Failed to get shard_stats array")
                        .iter()
                        .map(|shard_stats| {
                            let index_time_in_millis = &shard_stats["indexing"]
                                ["index_time_in_millis"]
                                .as_i64()
                                .expect("Failed to get index_time_in_millis");
                            let index_total = &shard_stats["indexing"]["index_total"]
                                .as_i64()
                                .expect("Failed to get index_total");
                            let total_size = &shard_stats["store"]["size_in_bytes"]
                                .as_i64()
                                .expect("Failed to get store.size_in_bytes");

                            let avg_docs_sec = match write_window_sec {
                                0 => 0,
                                x => index_total / x,
                            };
                            let avg_cpu_millis = match write_window_sec {
                                0 => 0,
                                x => index_time_in_millis / x,
                            };
                            let avg_mb_sec: f64 = match write_window_sec {
                                0 => 0.0,
                                x => (*total_size as f64 / 1_048_576.0) / (x as f64),
                            };

                            let indexing_patch = json!({
                                "shard": {
                                    "indexing": {
                                        "avg_docs_sec": avg_docs_sec,
                                        "avg_cpu_millis": avg_cpu_millis,
                                        "avg_mb_sec": avg_mb_sec,
                                    }
                                }
                            });

                            let mut doc = json!({
                                "shard":shard_stats,
                                "node": metadata.lookup.node.by_id(
                                    shard_stats["routing"]["node"].as_str().unwrap_or("")
                                ),
                            });
                            merge(&mut doc, &indexing_patch);
                            merge(&mut doc, &shard_doc);
                            doc
                        })
                        .collect();
                    shard_docs
                })
                .collect();

            let bytes_per_day_pri = match write_window_sec {
                0 => 0,
                x => {
                    (index_stats.primaries["store"]["size_in_bytes"]
                        .as_i64()
                        .expect("Failed to get primaries.store.size_in_bytes")
                        * 86_400)
                        / x
                }
            };

            let bytes_per_day_total = match write_window_sec {
                0 => 0,
                x => {
                    (index_stats.total["store"]["size_in_bytes"]
                        .as_i64()
                        .expect("Failed to get total.store.size_in_bytes")
                        * 86_400)
                        / x
                }
            };

            let index_patch = json!({
                "index": {
                    "alias": alias,
                    "data_stream": data_stream,
                    "name": index,
                    "primaries": {
                        "indexing": {
                            "est_bytes_per_day": bytes_per_day_pri,
                            "index_time_per_shard_in_millis": divide_values(
                                &index_stats.primaries["indexing"]["index_time_in_millis"],
                                &index_stats.primaries["shard_stats"]["total_count"],
                            ),
                        }
                    },
                    "total": {
                        "indexing": {
                            "est_bytes_per_day": bytes_per_day_total,
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
            merge(&mut doc, &index_patch);
            docs.insert(0, doc);
            docs
        })
        .collect();

    log::debug!("index_stats docs: {}", indices_stats.len());
    indices_stats
}
