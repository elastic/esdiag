use super::{DataProcessor, ElasticsearchMetadata, Lookups};
use crate::{
    data::elasticsearch::{IndexSettings, IndicesStats},
    processor::Metadata,
};
use json_patch::merge;
use rayon::prelude::*;
use serde_json::{json, Value};
use std::sync::Arc;

impl DataProcessor<Lookups, ElasticsearchMetadata> for IndicesStats {
    fn generate_docs(
        self,
        lookups: Arc<Lookups>,
        metadata: Arc<ElasticsearchMetadata>,
    ) -> (String, Vec<Value>) {
        let mut indices_stats = self.indices;
        log::debug!("index_stats indices: {}", indices_stats.len());
        let data_stream = "metrics-index-esdiag".to_string();
        let index_metadata = metadata.for_data_stream(&data_stream).as_meta_doc();
        let collection_date = metadata.timestamp;
        let lookup = lookups;

        let shard_metadata = metadata
            .clone()
            .for_data_stream("metrics-shard-esdiag")
            .as_meta_doc();

        let indices_stats: Vec<Value> = indices_stats
            .par_drain()
            .flat_map(|(index, ref mut index_stats)| {
                let mut shard_stats: Vec<_> = index_stats.shards.par_drain().collect();
                let data_stream = lookup.data_stream.by_id(&index);
                let alias = lookup.alias.by_name(&index);
                let ilm = lookup.ilm_explain.by_name(&index);
                let index_settings = match lookup.index_settings.by_name(&index) {
                    Some(settings) => settings,
                    None => &IndexSettings::default(),
                };
                let is_write_index = {
                    data_stream.is_some_and(|ds| ds.is_write_index)
                        || alias.is_some_and(|a| a.is_write_index)
                };

                let since_creation = index_settings
                    .creation_date
                    .map(|creation_date| collection_date - creation_date);
                let since_rollover = ilm
                    .and_then(|ilm| ilm.lifecycle_date_millis)
                    .map(|lifecycle_date| collection_date - lifecycle_date);
                let is_before_rollover = ilm.is_some_and(|ilm| {
                    ilm.action
                        .as_ref()
                        .is_some_and(|action| action == "rollover")
                });

                let write_phase_sec =
                    if let (Some(creation), Some(rollover)) = (since_creation, since_rollover) {
                        match creation > rollover {
                            true => (creation - rollover) / 1000,
                            false if is_before_rollover => creation / 1000,
                            _ => 0,
                        }
                    } else {
                        0
                    };

                let mut docs: Vec<_> = shard_stats
                    .par_drain(..)
                    .flat_map(|(shard_id, shard_stats)| {
                        let mut shard_doc = json!({
                            "index": {
                                "alias": alias,
                                "creation_date": index_settings.creation_date,
                                "codec": index_settings.codec,
                                "mode": index_settings.mode,
                                "source": index_settings.source,
                                "store": {
                                    "config": index_settings.store.as_ref().map(|store| store.get("config")),
                                },
                                "data_stream": data_stream,
                                "ilm": ilm,
                                "indexing_complete": index_settings.indexing_complete,
                                "is_write_index": is_write_index,
                                "name": index,
                                "since_creation": since_creation,
                                "since_rollover": since_rollover,
                                "uuid": index_stats.uuid,
                                "write_phase_sec": write_phase_sec,
                            },
                        });

                        merge(&mut shard_doc, &shard_metadata);
                        merge(&mut shard_doc, &json!({"shard": { "number": shard_id, }}));
                        let shard_docs: Vec<Value> = shard_stats
                            .as_array()
                            .expect("Failed to get shard_stats array")
                            .par_iter()
                            .map(|shard_stats| {
                                let node = lookup
                                    .node
                                    .by_id(shard_stats["routing"]["node"].as_str().unwrap_or(""));

                                // Indexing stats
                                let index_time_in_millis = &shard_stats["indexing"]
                                    ["index_time_in_millis"]
                                    .as_u64()
                                    .unwrap_or(0);
                                let index_total =
                                    &shard_stats["indexing"]["index_total"].as_u64().unwrap_or(0);
                                let total_size =
                                    &shard_stats["store"]["size_in_bytes"].as_u64().unwrap_or(0);
                                let bulk_size = &shard_stats["bulk"]["total_size_in_bytes"]
                                    .as_u64()
                                    .unwrap_or(0);

                                let avg_docs_sec = match write_phase_sec {
                                    0 => 0,
                                    x => index_total / x,
                                };
                                let index_avg_cpu_millis = match write_phase_sec {
                                    0 => 0,
                                    x => index_time_in_millis / x,
                                };
                                let indexing_avg_bytes_sec = match write_phase_sec {
                                    0 => 0,
                                    x => total_size / x,
                                };
                                let bulk_avg_bytes_sec = match write_phase_sec {
                                    0 => 0,
                                    x => bulk_size / x,
                                };
                                let storage_ratio = match bulk_size {
                                    0 => 0.0,
                                    x => *total_size as f32 / *x as f32,
                                };
                                let compression_ratio = *bulk_size as f32 / *total_size as f32;

                                // Search stats
                                let query_time_in_millis = &shard_stats["search"]
                                    ["query_time_in_millis"]
                                    .as_u64()
                                    .unwrap_or(0);
                                let query_total =
                                    &shard_stats["search"]["query_total"].as_f64().unwrap_or(0.0);
                                let fetch_time_in_millis = &shard_stats["search"]
                                    ["fetch_time_in_millis"]
                                    .as_u64()
                                    .unwrap_or(0);
                                let fetch_total =
                                    &shard_stats["search"]["fetch_total"].as_f64().unwrap_or(0.0);
                                let avg_query_cpu_millis = match since_creation {
                                    Some(x) => query_time_in_millis / (x / 1000),
                                    None => 0,
                                };
                                let avg_query_rate = match since_creation {
                                    Some(x) => query_total / (x as f64 / 1000.0),
                                    None => 0.0,
                                };
                                let avg_fetch_cpu_millis = match since_creation {
                                    Some(x) => fetch_time_in_millis / (x / 1000),
                                    None => 0,
                                };
                                let avg_fetch_rate = match since_creation {
                                    Some(x) => fetch_total / (x as f64 / 1000.0),
                                    None => 0.0,
                                };
                                let doc_count = &shard_stats["docs"]["count"].as_u64().unwrap_or(0);
                                let avg_doc_size = match total_size {
                                    size if *doc_count > 0 => *size / *doc_count,
                                    _ => 0,
                                };
                                let docs_per_gb = match total_size {
                                    0 => 0,
                                    1..1_073_741_824 if avg_doc_size > 0 => {
                                        1_073_741_824 / avg_doc_size
                                    }
                                    size => {
                                        (*doc_count as f32 / (*size as f32 / 1_073_741_824.0))
                                            as u64
                                    }
                                };
                                let docs_deleted_percent = {
                                    let deleted_count =
                                        shard_stats["docs"]["deleted"].as_f64().unwrap_or(0.0);
                                    deleted_count / (*doc_count as f64 + deleted_count)
                                };

                                // Patch new calculated stats
                                let indexing_patch = json!({
                                    "shard": {
                                        "docs": {
                                            "per_gb": docs_per_gb,
                                            "avg_size": avg_doc_size,
                                            "deleted_percent": docs_deleted_percent,
                                        },
                                        "indexing": {
                                            "avg_docs_sec": avg_docs_sec,
                                            "avg_cpu_millis": index_avg_cpu_millis,
                                            "avg_bytes_sec": indexing_avg_bytes_sec,
                                            "storage_ratio": storage_ratio,
                                        },
                                        "bulk": {
                                            "compression_ratio": compression_ratio,
                                            "avg_bytes_sec": bulk_avg_bytes_sec,
                                        },
                                        "search": {
                                            "avg_query_cpu_millis": avg_query_cpu_millis,
                                            "avg_query_rate": avg_query_rate,
                                            "avg_fetch_cpu_millis": avg_fetch_cpu_millis,
                                            "avg_fetch_rate": avg_fetch_rate,
                                        }
                                    }
                                });

                                let mut doc = json!({
                                    "shard": shard_stats,
                                    "node": node,
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
                        index_stats.primaries["store"]["size_in_bytes"]
                            .as_u64()
                            .expect(&format!(
                                "Failed to parse {index} primaries.store.size_in_bytes"
                            ))
                            * (86_400 / x)
                    }
                };

                let bytes_per_day_total = match write_phase_sec {
                    0 => 0,
                    x => {
                        (index_stats.total["store"]["size_in_bytes"]
                            .as_u64()
                            .unwrap_or(0)
                            * 86_400)
                            / x
                    }
                };
                let index_size = index_stats.total["store"]["size_in_bytes"]
                    .as_u64()
                    .unwrap_or(0);
                let doc_count = index_stats.total["docs"]["count"].as_u64().unwrap_or(0);
                let doc_deleted = index_stats.total["docs"]["deleted"].as_f64().unwrap_or(0.0);
                let avg_doc_size = match index_size {
                    size if doc_count > 0 => size / doc_count,
                    _ => 0,
                };
                let docs_per_gb = match index_size {
                    0 => 0,
                    1..1_073_741_824 if avg_doc_size > 0 => 1_073_741_824 / avg_doc_size,
                    size => (doc_count as f32 / (size as f32 / 1_073_741_824.0)) as u64,
                };
                let docs_deleted_percent = doc_deleted / (doc_count as f64 + doc_deleted);

                let index_patch = json!({
                    "index": {
                        "alias": alias,
                        "codec": index_settings.codec,
                        "mode": index_settings.mode,
                        "source": index_settings.source,
                        "store": {
                            "config": index_settings.store.as_ref().map(|store| store.get("config")),
                        },
                        "data_stream": data_stream,
                        "ilm": ilm,
                        "name": index,
                        "is_write_index": is_write_index,
                        "since_creation": since_creation,
                        "since_rollover": since_rollover,
                        "write_phase_sec": write_phase_sec,
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
                            "docs": {
                                "per_gb": docs_per_gb,
                                "avg_size": avg_doc_size,
                                "deleted_percent": docs_deleted_percent,
                            },
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
                merge(&mut doc, &index_metadata);
                merge(&mut doc, &index_patch);
                docs.insert(0, doc);
                docs
            })
            .collect();

        log::debug!("index_stats docs: {}", indices_stats.len());
        (data_stream, indices_stats)
    }
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
