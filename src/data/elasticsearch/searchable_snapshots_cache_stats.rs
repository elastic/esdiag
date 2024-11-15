use crate::data::{
    diagnostic::{elasticsearch::DataSet, DataSource},
    Uri,
};
use color_eyre::eyre::{eyre, Result};
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

impl DataSource for SearchableSnapshotsCacheStats {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => {
                Ok("commercial/searchable_snapshots_cache_stats.json")
            }
            Uri::Host(_) | Uri::Url(_) => Ok("_searchable_snapshots/cache/stats"),
            _ => Err(eyre!(
                "Unsupported source for searchable snapshots cache stats"
            )),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::SearchableSnapshotsCacheStats)
    }
}
