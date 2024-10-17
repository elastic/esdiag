use super::Lookup;
use crate::data::elasticsearch::{SearchableSnapshotsCacheStats, SharedCacheStats};

impl From<&String> for Lookup<SharedCacheStats> {
    fn from(string: &String) -> Self {
        let nodes: SearchableSnapshotsCacheStats =
            serde_json::from_str(&string).expect("Failed to parse SharedCacheStats");
        Lookup::<SharedCacheStats>::from(nodes)
    }
}

impl From<SearchableSnapshotsCacheStats> for Lookup<SharedCacheStats> {
    fn from(mut searchable_snapshots_cache_stats: SearchableSnapshotsCacheStats) -> Self {
        let mut lookup: Lookup<SharedCacheStats> = Lookup::new();

        searchable_snapshots_cache_stats
            .nodes
            .drain()
            .for_each(|(node_id, node)| {
                lookup.add(node.shared_cache).with_id(&node_id);
            });

        log::debug!("lookup shared_cache entries: {}", lookup.len(),);
        lookup
    }
}
