use super::metadata::Metadata;
use crate::processor::elasticsearch::lookup::index::IndexData;
use json_patch::merge;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{from_value, json, Map, Value};
use std::collections::HashMap;

pub fn enrich(metadata: &Metadata, mut data: Value) -> Vec<Value> {
    let lookup = &metadata.lookup;
    let metadata = &metadata.as_doc;
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
        "@timestamp": metadata.timestamp,
        "cluster": metadata.cluster,
        "diagnostic": metadata.diagnostic,
        "data_stream": {
            "dataset": "shard",
            "namespace": "esdiag",
            "type": "metrics",
        }
    });

    let data_stream_index_patch = json!({
        "@timestamp": metadata.timestamp,
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
            let data_stream = lookup.data_stream.by_name(index.as_str());
            let alias = lookup.alias.by_name(index.as_str());
            let ilm = lookup.ilm.by_name(index);
            let index_data = lookup.index.by_name(index).unwrap_or(&IndexData {
                indexing_complete: None,
                creation_date: None,
            });
            let is_write_index = {
                data_stream.is_some_and(|ds| ds.is_write_index())
                    || alias.is_some_and(|a| a.is_write_index())
            };

            let since_creation = index_data
                .creation_date
                .map(|date| metadata.diagnostic.collection_date - date);
            let since_rollover = ilm
                .and_then(|ilm| ilm.lifecycle_date_millis)
                .map(|date| metadata.diagnostic.collection_date - date);
            let is_before_rollover = ilm.is_some_and(|ilm| {
                ilm.action
                    .as_ref()
                    .is_some_and(|action| action == "rollover")
            });

            let write_phase_sec =
                if let (Some(creation), Some(rollover)) = (since_creation, since_rollover) {
                    match creation == rollover {
                        true if is_before_rollover => creation / 1000,
                        true => 0,
                        false => (rollover - creation) / 1000,
                    }
                } else {
                    0
                };

            let mut docs: Vec<_> = shard_stats
                .par_iter()
                .flat_map(|(shard_id, shard_stats)| {
                    let mut shard_doc = json!({
                        "index": {
                            "alias": alias,
                            "creation_date": index_data.creation_date,
                            "data_stream": data_stream,
                            "ilm": ilm,
                            "indexing_complete": index_data.indexing_complete,
                            "is_write_index": is_write_index,
                            "name": index,
                            "since_creation": since_creation,
                            "since_rollover": since_rollover,
                            "uuid": index_stats.uuid,
                            "write_phase_sec": write_phase_sec,
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
                            let bulk_size = &shard_stats["bulk"]["total_size_in_bytes"]
                                .as_i64()
                                .unwrap_or(0);

                            let avg_docs_sec = match write_phase_sec {
                                0 => 0,
                                x => index_total / x,
                            };
                            let avg_cpu_millis = match write_phase_sec {
                                0 => 0,
                                x => index_time_in_millis / x,
                            };
                            let indexing_avg_bytes_sec = match write_phase_sec {
                                0 => 0,
                                x => *total_size / x,
                            };
                            let bulk_avg_bytes_sec = {
                                let time = shard_stats["bulk"]["total_time_in_millis"].as_i64();
                                match time {
                                    Some(time) if time / 1000 > 0 => bulk_size / (time / 1000),
                                    _ => 0,
                                }
                            };

                            let indexing_patch = json!({
                                "shard": {
                                    "indexing": {
                                        "avg_docs_sec": avg_docs_sec,
                                        "avg_cpu_millis": avg_cpu_millis,
                                        "avg_bytes_sec": indexing_avg_bytes_sec,
                                    },
                                    "bulk": {
                                        "avg_bytes_sec": bulk_avg_bytes_sec,
                                    }
                                }
                            });

                            let mut doc = json!({
                                "shard":shard_stats,
                                "node": lookup.node.by_id(
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

            let bytes_per_day_pri = match write_phase_sec {
                0 => 0,
                x => {
                    (index_stats.primaries["store"]["size_in_bytes"]
                        .as_i64()
                        .expect("Failed to get primaries.store.size_in_bytes")
                        * 86_400)
                        / x
                }
            };

            let bytes_per_day_total = match write_phase_sec {
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
                    "is_write_index": is_write_index,
                    "ilm": ilm,
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

// Serializing data structures

#[derive(Serialize)]
pub struct ShardDoc {
    alias: Option<String>,
    creation_date: Option<i64>,
    data_stream: Option<String>,
    ilm: Option<String>,
    index: String,
    indexing_complete: Option<bool>,
    is_write_index: Option<bool>,
    since_creation: Option<i64>,
    since_rollover: Option<i64>,
    uuid: String,
    write_window_sec: Option<i64>,
}

impl ShardDoc {
    pub fn new(index: &str) -> Self {
        ShardDoc {
            alias: None,
            creation_date: None,
            data_stream: None,
            ilm: None,
            index: index.to_string(),
            indexing_complete: None,
            is_write_index: None,
            since_creation: None,
            since_rollover: None,
            uuid: String::new(),
            write_window_sec: None,
        }
    }
}

#[derive(Serialize)]
pub struct IndexStatsDoc {}

// Deserializing data structures

#[derive(Deserialize, Serialize)]
struct IndexStats {
    uuid: String,
    health: Option<String>,
    primaries: Value,
    total: Value,
    shards: Map<String, Value>,
}

// Supporting functions

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
