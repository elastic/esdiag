// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::DataSource;
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

    fn name() -> String {
        "searchable_snapshots_cache_stats".to_string()
    }
}
