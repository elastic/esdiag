use crate::data::{diagnostic::data_source::DataSource, Uri};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Deserialize, Serialize)]
pub struct IndicesStats {
    _shards: Value,
    _all: Value,
    pub indices: HashMap<String, IndexStats>,
}

#[derive(Deserialize, Serialize)]
pub struct IndexStats {
    pub uuid: Option<String>,
    pub health: Option<String>,
    pub primaries: Value,
    pub total: Value,
    #[serde(skip_serializing)]
    pub shards: HashMap<String, Value>,
}

impl DataSource for IndicesStats {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("indices_stats.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_index/stats?level=shards"),
            _ => Err(eyre!("Unsuppored source for indices stats")),
        }
    }
}
