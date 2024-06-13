use super::{Identifiers, Lookup};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SharedCache {
    shared_cache: SharedCacheStats,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Nodes {
    nodes: HashMap<String, SharedCache>,
}

impl From<&String> for Lookup<SharedCacheStats> {
    fn from(string: &String) -> Self {
        let nodes: Nodes = serde_json::from_str(&string).expect("Failed to parse SharedCacheStats");
        let mut lookup_shared_cache: Lookup<SharedCacheStats> = Lookup::new();

        for (node_id, node) in nodes.nodes {
            let ids = Identifiers {
                id: Some(node_id.clone()),
                name: None,
                host: None,
                ip: None,
            };
            lookup_shared_cache.insert(ids, node.shared_cache);
        }
        log::debug!(
            "lookup_shared_cache entries: {}",
            lookup_shared_cache.entries.len(),
        );
        lookup_shared_cache
    }
}
