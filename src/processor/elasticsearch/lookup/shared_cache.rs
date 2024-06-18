use super::{Lookup, LookupDisplay};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

impl From<&String> for Lookup<SharedCacheStats> {
    fn from(string: &String) -> Self {
        let nodes: Nodes = serde_json::from_str(&string).expect("Failed to parse SharedCacheStats");
        let mut lookup_shared_cache: Lookup<SharedCacheStats> = Lookup::new();

        for (node_id, node) in nodes.nodes {
            lookup_shared_cache.add(node.shared_cache).with_id(&node_id);
        }

        log::debug!(
            "lookup_shared_cache entries: {}",
            lookup_shared_cache.entries.len(),
        );
        lookup_shared_cache
    }
}

impl LookupDisplay for SharedCacheStats {
    fn display() -> &'static str {
        "shared_cache_stats"
    }
}

#[derive(Clone, Deserialize)]
struct Nodes {
    nodes: HashMap<String, SharedCache>,
}

#[derive(Clone, Deserialize)]
struct SharedCache {
    shared_cache: SharedCacheStats,
}
