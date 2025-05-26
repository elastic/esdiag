use crate::data::diagnostic::{DataSource, data_source::PathType, elasticsearch::DataSet};
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct IndicesStats {
    _shards: Value,
    _all: Value,
    pub indices: HashMap<String, IndexStats>,
}

#[derive(Deserialize)]
pub struct IndexStats {
    pub uuid: Option<String>,
    pub health: Option<String>,
    pub primaries: Stats,
    pub total: Stats,
    #[serde(skip_serializing)]
    pub shards: Option<HashMap<String, Value>>,
}

#[derive(Deserialize, Serialize)]
pub struct Stats {
    pub bulk: Option<Bulk>,
    pub completion: Option<Completion>,
    pub dense_vector: Option<DenseVector>,
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
    pub sparse_vector: Option<SparseVector>,
    pub store: Option<StoreStats>,
    pub translog: Option<Translog>,
    pub warmer: Option<Warmer>,
}

#[derive(Deserialize, Serialize)]
pub struct Docs {
    pub count: Option<u64>,
    pub deleted: Option<u64>,
    pub total_size_in_bytes: Option<u64>,
}

#[derive(Deserialize, Serialize)]
pub struct ShardStats {
    pub total_count: u64,
}

#[derive(Deserialize, Serialize)]
pub struct StoreStats {
    pub size_in_bytes: u64,
    pub total_data_set_size_in_bytes: Option<u64>,
    pub reserved_in_bytes: Option<u64>,
}

#[derive(Deserialize, Serialize)]
pub struct Indexing {
    pub delete_current: Option<u64>,
    pub delete_time_in_millis: Option<u64>,
    pub delete_total: Option<u64>,
    pub index_current: Option<u64>,
    pub index_failed: Option<u64>,
    pub index_time_in_millis: Option<u64>,
    pub index_total: Option<u64>,
    pub is_throttled: Option<bool>,
    pub noop_update_total: Option<u64>,
    pub throttle_time_in_millis: Option<u64>,
    pub write_load: Option<f64>,
}

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

#[derive(Deserialize, Serialize)]
pub struct Search {
    fetch_current: Option<u64>,
    fetch_failure: Option<u64>,
    fetch_time_in_millis: Option<u64>,
    fetch_total: Option<u64>,
    open_contexts: Option<u64>,
    query_current: Option<u64>,
    query_failure: Option<u64>,
    query_time_in_millis: Option<u64>,
    query_total: Option<u64>,
    scroll_current: Option<u64>,
    scroll_time_in_millis: Option<u64>,
    scroll_total: Option<u64>,
    suggest_current: Option<u64>,
    suggest_time_in_millis: Option<u64>,
    suggest_total: Option<u64>,
}

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

#[derive(Deserialize, Serialize)]
pub struct Refresh {
    external_total: Option<u64>,
    external_total_time_in_millis: Option<u64>,
    listeners: Option<u64>,
    total: Option<u64>,
    total_time_in_millis: Option<u64>,
}

#[derive(Deserialize, Serialize)]
pub struct Flush {
    total: Option<u64>,
    periodic: Option<u64>,
    total_time_in_millis: Option<u64>,
    total_time_excluding_waiting_on_lock_in_millis: Option<u64>,
}

#[derive(Deserialize, Serialize)]
pub struct Warmer {
    current: Option<u64>,
    total: Option<u64>,
    total_time_in_millis: Option<u64>,
}

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

#[derive(Deserialize, Serialize)]
pub struct Fielddata {
    memory_size_in_bytes: Option<u64>,
    evictions: Option<u64>,
    global_ordinals: Option<Value>,
}

#[derive(Deserialize, Serialize)]
pub struct Completion {
    size_in_bytes: Option<u64>,
}

#[derive(Deserialize, Serialize)]
pub struct Segments {
    count: Option<u64>,
    doc_values_memory_in_bytes: Option<u64>,
    // file_sizes
    fixed_bit_set_memory_in_bytes: Option<u64>,
    index_writer_memory_in_bytes: Option<u64>,
    max_unsafe_auto_id_timestamp: Option<i64>,
    memory_in_bytes: Option<u64>,
    norms_memory_in_bytes: Option<u64>,
    points_memory_in_bytes: Option<u64>,
    stored_fields_memory_in_bytes: Option<u64>,
    term_vectors_memory_in_bytes: Option<u64>,
    terms_memory_in_bytes: Option<u64>,
    version_map_memory_in_bytes: Option<u64>,
}

#[derive(Deserialize, Serialize)]
pub struct Translog {
    operations: Option<u64>,
    size_in_bytes: Option<u64>,
    uncommitted_operations: Option<u64>,
    uncommitted_size_in_bytes: Option<u64>,
    earliest_last_modified_age: Option<u64>,
}

#[derive(Deserialize, Serialize)]
pub struct RequestCache {
    memory_size_in_bytes: Option<u64>,
    evictions: Option<u64>,
    hit_count: Option<u64>,
    miss_count: Option<u64>,
}

#[derive(Deserialize, Serialize)]
pub struct Recovery {
    current_as_source: Option<u64>,
    current_as_target: Option<u64>,
    throttle_time_in_millis: Option<u64>,
}

#[derive(Deserialize, Serialize)]
pub struct Bulk {
    pub avg_size_in_bytes: Option<u64>,
    pub avg_time_in_millis: Option<u64>,
    pub total_operations: Option<u64>,
    pub total_size_in_bytes: Option<u64>,
    pub total_time_in_millis: Option<u64>,
}

#[derive(Deserialize, Serialize)]
pub struct DenseVector {
    value_count: Option<u64>,
}

#[derive(Deserialize, Serialize)]
pub struct SparseVector {
    value_count: Option<u64>,
}

impl DataSource for IndicesStats {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("indices_stats.json"),
            PathType::Url => Ok("_all/_stats?level=shards"),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::IndicesStats)
    }
}
