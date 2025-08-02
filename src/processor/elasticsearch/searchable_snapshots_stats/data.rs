use super::super::super::diagnostic::data_source::PathType;
use super::super::DataSource;
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize)]
pub struct SearchableSnapshotsStats {
    pub _shards: Value,
    pub total: Vec<Value>,
    pub indices: HashMap<String, Total>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Total {
    pub total: Vec<Value>,
}

impl DataSource for SearchableSnapshotsStats {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("commercial/searchable_snapshots_stats.json"),
            PathType::Url => Ok("_searchable_snapshots/stats"),
        }
    }

    fn name() -> String {
        "searchable_snapshots_stats".to_string()
    }
}
