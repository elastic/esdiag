// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{super::Lookup, SearchableSnapshotsCacheStats, SharedCacheStats};
use eyre::Result;

impl From<&String> for Lookup<SharedCacheStats> {
    fn from(string: &String) -> Self {
        match serde_json::from_str::<SearchableSnapshotsCacheStats>(string) {
            Ok(nodes) => Lookup::<SharedCacheStats>::from_parsed(nodes),
            Err(e) => {
                tracing::warn!("Failed to parse SearchableSnapshotsCacheStats: {}", e);
                Lookup::new()
            }
        }
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

        tracing::debug!("lookup shared_cache entries: {}", lookup.len(),);
        lookup
    }
}

impl From<Result<SearchableSnapshotsCacheStats>> for Lookup<SharedCacheStats> {
    fn from(stats_result: Result<SearchableSnapshotsCacheStats>) -> Self {
        match stats_result {
            Ok(stats) => Lookup::<SharedCacheStats>::from_parsed(stats),
            Err(e) => {
                tracing::warn!("Failed to parse SearchableSnapshotsCacheStats: {}", e);
                Lookup::new()
            }
        }
    }
}
