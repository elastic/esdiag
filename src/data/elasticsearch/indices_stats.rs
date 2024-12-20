use crate::data::diagnostic::{data_source::PathType, elasticsearch::DataSet, DataSource};
use color_eyre::eyre::Result;
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
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("indices_stats.json"),
            PathType::Url => Ok("_index/stats?level=shards"),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::IndicesStats)
    }
}
