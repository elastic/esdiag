// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::data_source::{PathType, StreamingDataSource};
use super::super::DataSource;
use crate::data::{i64_from_string, map_as_vec_entries};
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;

#[derive(Deserialize)]
pub struct IndicesStats {
    pub _shards: ShardsStats,
    // _all: Value,
    #[serde(deserialize_with = "map_as_vec_entries")]
    pub indices: Vec<(String, IndexStats)>,
}

impl StreamingDataSource for IndicesStats {
    type Item = (String, IndexStats);

    fn deserialize_stream<'de, D>(
        deserializer: D,
        sender: Sender<Result<Self::Item>>,
    ) -> std::result::Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct IndicesStatsVisitor {
            sender: Sender<Result<(String, IndexStats)>>,
        }

        impl<'de> serde::de::Visitor<'de> for IndicesStatsVisitor {
            type Value = ();

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("IndicesStats object")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                while let Some(key) = map.next_key::<String>()? {
                    if key == "indices" {
                        map.next_value_seed(IndicesMapVisitor {
                            sender: self.sender.clone(),
                        })?;
                    } else {
                        let _ = map.next_value::<serde::de::IgnoredAny>()?;
                    }
                }
                Ok(())
            }
        }

        struct IndicesMapVisitor {
            sender: Sender<Result<(String, IndexStats)>>,
        }

        impl<'de> serde::de::DeserializeSeed<'de> for IndicesMapVisitor {
            type Value = ();
            fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_map(self)
            }
        }

        impl<'de> serde::de::Visitor<'de> for IndicesMapVisitor {
            type Value = ();
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("indices map")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                while let Some(key) = map.next_key::<String>()? {
                    let value = map.next_value::<IndexStats>()?;
                    if self.sender.blocking_send(Ok((key, value))).is_err() {
                        return Ok(());
                    }
                }
                Ok(())
            }
        }

        deserializer.deserialize_map(IndicesStatsVisitor { sender })
    }
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct ShardsStats {
    total: Option<u32>,
    successful: Option<u32>,
    failed: Option<u32>,
}

#[skip_serializing_none]
#[derive(Deserialize)]
pub struct IndexStats {
    pub uuid: Option<String>,
    pub health: Option<String>,
    pub primaries: Stats,
    pub total: Stats,
    pub shards: Option<HashMap<u16, Vec<ShardEntry>>>,
}

#[skip_serializing_none]
#[derive(Deserialize)]
pub struct ShardEntry {
    pub routing: ShardRouting,
    pub commit: ShardCommit,
    pub seq_no: SequenceNumber,
    pub retention_leases: RetentionLeases,
    pub shard_path: Option<ShardPath>,
    pub search_idle: Option<bool>,
    pub search_idle_time: Option<u64>,
    #[serde(flatten)]
    pub stats: Stats,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct SequenceNumber {
    max_seq_no: Option<i64>,
    local_checkpoint: Option<i64>,
    global_checkpoint: Option<i64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct ShardPath {
    state_path: Option<String>,
    data_path: Option<String>,
    is_custom_data_path: Option<bool>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct RetentionLeases {
    primary_term: Option<u64>,
    version: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct VectorCount {
    pub value_count: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct ShardCommit {
    id: Option<String>,
    generation: Option<u64>,
    user_data: Option<HashMap<String, String>>,
    num_docs: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct ShardRouting {
    pub node: String,
    pub primary: bool,
    pub state: String,
    pub relocating_node: Option<String>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Stats {
    pub bulk: Option<Bulk>,
    pub completion: Option<Completion>,
    pub dense_vector: Option<VectorCount>,
    pub docs: Option<Docs>,
    pub fielddata: Option<Fielddata>,
    pub flush: Option<Flush>,
    pub get: Option<Get>,
    pub indexing: Option<Indexing>,
    pub merges: Option<Merges>,
    pub query_cache: Option<QueryCache>,
    pub recovery: Option<Recovery>,
    pub refresh: Option<Refresh>,
    pub request_cache: Option<RequestCache>,
    pub search: Option<Search>,
    pub segments: Option<Segments>,
    pub shard_stats: ShardStats,
    pub sparse_vector: Option<VectorCount>,
    pub store: Option<StoreStats>,
    pub translog: Option<Translog>,
    pub warmer: Option<Warmer>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Docs {
    pub count: u64,
    pub deleted: u64,
    pub total_size_in_bytes: Option<u64>,
    // Calculated
    pub avg_size: Option<u64>,
    pub per_gb: Option<u64>,
    pub deleted_percent: Option<f32>,
}

#[derive(Deserialize, Serialize)]
pub struct ShardStats {
    pub total_count: u64,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct StoreStats {
    pub size_in_bytes: u64,
    pub total_data_set_size_in_bytes: Option<u64>,
    pub reserved_in_bytes: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Indexing {
    pub delete_current: u64,
    pub delete_time_in_millis: u64,
    pub delete_total: u64,
    pub index_current: u64,
    pub index_failed: u64,
    pub index_time_in_millis: u64,
    pub index_total: u64,
    pub is_throttled: Option<bool>,
    pub noop_update_total: Option<u64>,
    pub throttle_time_in_millis: Option<u64>,
    pub write_load: Option<f64>,
    // Calculated
    pub avg_docs_sec: Option<u64>,
    pub avg_cpu_millis: Option<u64>,
    pub avg_bytes_sec: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Get {
    current: Option<u64>,
    exists_time_in_millis: Option<u64>,
    exists_total: Option<u64>,
    missing_time_in_millis: Option<u64>,
    missing_total: Option<u64>,
    time_in_millis: Option<u64>,
    total: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Search {
    pub fetch_current: Option<u64>,
    pub fetch_failure: Option<u64>,
    pub fetch_time_in_millis: u64,
    pub fetch_total: u64,
    pub open_contexts: Option<u64>,
    pub query_current: Option<u64>,
    pub query_failure: Option<u64>,
    pub query_time_in_millis: u64,
    pub query_total: u64,
    pub scroll_current: Option<u64>,
    pub scroll_time_in_millis: Option<u64>,
    pub scroll_total: Option<u64>,
    pub suggest_current: Option<u64>,
    pub suggest_time_in_millis: Option<u64>,
    pub suggest_total: Option<u64>,
    // Calculated
    pub avg_fetch_cpu_millis: Option<u64>,
    pub avg_fetch_rate: Option<f64>,
    pub avg_query_cpu_millis: Option<u64>,
    pub avg_query_rate: Option<f64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Merges {
    current: Option<u64>,
    current_docs: Option<u64>,
    current_size_in_bytes: Option<u64>,
    total: Option<u64>,
    total_time_in_millis: Option<u64>,
    total_auto_throttle_in_bytes: Option<u64>,
    total_docs: Option<u64>,
    total_size_in_bytes: Option<u64>,
    total_stopped_time_in_millis: Option<u64>,
    total_throttled_time_in_millis: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Refresh {
    external_total: Option<u64>,
    external_total_time_in_millis: Option<u64>,
    listeners: Option<u64>,
    total: Option<u64>,
    total_time_in_millis: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Flush {
    total: Option<u64>,
    periodic: Option<u64>,
    total_time_in_millis: Option<u64>,
    total_time_excluding_waiting_on_lock_in_millis: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Warmer {
    current: Option<u64>,
    total: Option<u64>,
    total_time_in_millis: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct QueryCache {
    cache_count: Option<u64>,
    cache_size: Option<u64>,
    evictions: Option<u64>,
    hit_count: Option<u64>,
    memory_size_in_bytes: Option<u64>,
    miss_count: Option<u64>,
    total_count: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Fielddata {
    memory_size_in_bytes: Option<u64>,
    evictions: Option<u64>,
    global_ordinals: Option<GlobalOrdinals>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct GlobalOrdinals {
    build_time_in_millis: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Completion {
    size_in_bytes: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Segments {
    count: Option<u64>,
    doc_values_memory_in_bytes: Option<u64>,
    // file_sizes
    fixed_bit_set_memory_in_bytes: Option<u64>,
    index_writer_memory_in_bytes: Option<u64>,
    #[serde(deserialize_with = "i64_from_string")]
    max_unsafe_auto_id_timestamp: Option<i64>,
    memory_in_bytes: Option<u64>,
    norms_memory_in_bytes: Option<u64>,
    points_memory_in_bytes: Option<u64>,
    stored_fields_memory_in_bytes: Option<u64>,
    term_vectors_memory_in_bytes: Option<u64>,
    terms_memory_in_bytes: Option<u64>,
    version_map_memory_in_bytes: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Translog {
    operations: Option<u64>,
    size_in_bytes: Option<u64>,
    uncommitted_operations: Option<u64>,
    uncommitted_size_in_bytes: Option<u64>,
    earliest_last_modified_age: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct RequestCache {
    memory_size_in_bytes: Option<u64>,
    evictions: Option<u64>,
    hit_count: Option<u64>,
    miss_count: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Recovery {
    current_as_source: Option<u64>,
    current_as_target: Option<u64>,
    throttle_time_in_millis: Option<u64>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Bulk {
    pub avg_size_in_bytes: Option<u64>,
    pub avg_time_in_millis: Option<u64>,
    pub total_operations: u64,
    pub total_size_in_bytes: u64,
    pub total_time_in_millis: u64,
    // Calculated
    pub avg_bytes_sec: Option<u64>,
    pub compression_ratio: Option<f32>,
    pub storage_ratio: Option<f32>,
}

impl DataSource for IndicesStats {
    fn source(path: PathType, version: Option<&semver::Version>) -> Result<String> {
        let name = Self::name();
        if let Ok(source_conf) =
            crate::processor::diagnostic::data_source::get_source(Self::product(), &name)
        {
            match path {
                PathType::File => Ok(source_conf.get_file_path(&name)),
                PathType::Url => {
                    let v = version.ok_or_else(|| eyre::eyre!("Version required for URL"))?;
                    source_conf.get_url(v)
                }
            }
        } else {
            // Fallback for missing or not-yet-supported sources
            eyre::bail!(
                "Source configuration missing for product: {}, name: {}",
                Self::product(),
                name
            )
        }
    }

    fn name() -> String {
        "indices_stats".to_string()
    }
}
