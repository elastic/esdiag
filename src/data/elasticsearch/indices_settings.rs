use super::DataStream;
use crate::data::{diagnostic::data_source::DataSource, Uri};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub type IndicesSettings = HashMap<String, Settings>;

#[derive(Clone, Deserialize, Serialize)]
pub struct IndexSettings {
    pub allocation: Option<Value>,
    pub auto_expand_replicas: Option<String>,
    pub blocks: Option<Value>,
    #[serde(default = "default_codec")]
    pub codec: String,
    #[serde(deserialize_with = "number_from_string")]
    pub creation_date: Option<i64>,
    pub default_pipeline: Option<String>,
    pub final_pipeline: Option<String>,
    pub hidden: Option<String>,
    pub lifecycle: Option<Value>,
    pub mapping: Option<Value>,
    #[serde(deserialize_with = "number_from_string")]
    pub number_of_replicas: Option<i64>,
    #[serde(deserialize_with = "number_from_string")]
    pub number_of_shards: Option<i64>,
    pub priority: Option<String>,
    pub provided_name: String,
    pub query: Option<Value>,
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval: String,
    pub routing: Option<Value>,
    pub shard: Option<Value>,
    pub shard_limit: Option<Value>,
    pub store: Option<Value>,
    pub sort: Option<Value>,
    pub uuid: String,
    pub version: Value,
    // Not in source json
    #[serde(skip_deserializing)]
    pub age: Option<i64>,
    #[serde(skip_deserializing)]
    pub data_stream: Option<DataStream>,
    #[serde(skip_deserializing)]
    pub name: Option<String>,
}

impl IndexSettings {
    /// Returns `true` if indexing_complete is true
    pub fn indexing_complete(&self) -> Option<bool> {
        if let Some(lifecycle) = &self.lifecycle {
            if let Some(Value::String(s)) = lifecycle.get("indexing_complete") {
                return Some(s == "true");
            }
        }
        None
    }
}

fn default_codec() -> String {
    String::from("best_speed")
}

fn default_refresh_interval() -> String {
    String::from("default")
}

#[derive(Deserialize)]
pub struct Settings {
    settings: Index,
}

impl Settings {
    /// Consume `self` and return the index settings, dropping the unnecessary parent data.
    pub fn index(self) -> IndexSettings {
        self.settings.index
    }
}

#[derive(Deserialize)]
struct Index {
    index: IndexSettings,
}

/// The standard deserializer from serde_json does not deserializing numbers from
/// strings. Unfortunately the _settings API frequently wraps numbers in quotes.

fn number_from_string<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;

    match value {
        Value::Number(num) => Ok(num.as_i64()),
        Value::String(s) => Ok(s.parse::<i64>().ok()),
        Value::Null => Ok(None),
        _ => Err(serde::de::Error::custom(
            "expected a number or a string representing a number",
        )),
    }
}

impl DataSource for IndicesSettings {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("settings.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_all/_settings"),
            _ => Err(eyre!("Unsuppored source for index settings")),
        }
    }
}
