use super::super::super::diagnostic::data_source::PathType;
use super::super::DataSource;
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Deserialize, Serialize)]
pub struct SearchableSnapshotsCacheStats {
    pub nodes: HashMap<String, SharedCache>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct SharedCache {
    pub shared_cache: SharedCacheStats,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct SharedCacheStats {
    pub reads: u64,
    pub bytes_read_in_bytes: u64,
    pub writes: u64,
    pub bytes_written_in_bytes: u64,
    pub evictions: u64,
    pub num_regions: u64,
    pub size_in_bytes: u64,
    pub region_size_in_bytes: u64,
}

impl DataSource for SearchableSnapshotsCacheStats {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("commercial/searchable_snapshots_cache_stats.json"),
            PathType::Url => Ok("_searchable_snapshots/cache/stats"),
        }
    }

    fn name() -> String {
        "searchable_snapshots_cache_stats".to_string()
    }
}
