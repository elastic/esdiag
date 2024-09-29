use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Deserialize)]
pub struct SearchableSnapshotsCacheStats {
    pub nodes: HashMap<String, SharedCache>,
}

#[derive(Clone, Deserialize)]
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
