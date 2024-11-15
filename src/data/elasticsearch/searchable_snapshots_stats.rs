use crate::data::{
    diagnostic::{elasticsearch::DataSet, DataSource},
    Uri,
};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchableSnapshotsStats {
    pub _shards: Value,
    pub total: Vec<Value>,
    pub indices: HashMap<String, Total>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Total {
    pub total: Vec<Value>,
}

impl DataSource for SearchableSnapshotsStats {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("commercial/searchable_snapshots_stats.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_searchable_snapshots/stats"),
            _ => Err(eyre!("Unsupported source for searchable snapshots stats")),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::SearchableSnapshotsStats)
    }
}
