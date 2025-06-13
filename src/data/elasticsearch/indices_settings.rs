use super::DataStream;
use crate::data::diagnostic::{DataSource, data_source::PathType, elasticsearch::DataSet};
use eyre::Result;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use serde_with::skip_serializing_none;
use std::collections::HashMap;

pub type IndicesSettings = HashMap<String, Settings>;

#[skip_serializing_none]
#[derive(Clone, Deserialize, Serialize)]
pub struct IndexSettings {
    pub allocation: Option<Value>,
    pub auto_expand_replicas: Option<String>,
    pub blocks: Option<Value>,
    #[serde(default = "default_to_default", deserialize_with = "deserialize_codec")]
    pub codec: String,
    #[serde(deserialize_with = "number_from_string")]
    pub creation_date: Option<u64>,
    pub default_pipeline: Option<String>,
    pub final_pipeline: Option<String>,
    pub hidden: Option<String>,
    #[serde(default)]
    pub is_write_index: bool,
    pub lifecycle: Option<Value>,
    pub mapping: Option<Value>,
    #[serde(default = "default_to_standard")]
    pub mode: String,
    #[serde(deserialize_with = "number_from_string")]
    pub number_of_replicas: Option<u64>,
    #[serde(deserialize_with = "number_from_string")]
    pub number_of_shards: Option<u64>,
    pub priority: Option<String>,
    pub provided_name: Option<String>,
    pub query: Option<Value>,
    #[serde(default = "default_to_default")]
    pub refresh_interval: String,
    pub routing: Option<Value>,
    pub shard: Option<Value>,
    pub shard_limit: Option<Value>,
    pub sort: Option<Value>,
    pub source: Option<String>,
    pub store: Option<StoreSettings>,
    pub uuid: String,
    pub version: Value,
    // Not in source json
    #[serde(skip_deserializing)]
    pub age: Option<u64>,
    #[serde(skip_deserializing)]
    pub data_stream: Option<DataStream>,
    #[serde(skip_deserializing)]
    pub name: Option<String>,
}

#[skip_serializing_none]
#[derive(Clone, Serialize, Deserialize)]
pub struct StoreSettings {
    pub config: Option<String>,
    pub store_type: Option<String>,
    pub snapshot: Option<StoreSnapshot>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StoreSnapshot {
    pub snapshot_name: String,
    pub index_uuid: String,
    pub repository_uuid: String,
    pub index_name: String,
    pub partial: String,
    pub repository_name: String,
    pub snapshot_uuid: String,
}

impl IndexSettings {
    /// Determines additional field values from previously deserialized data
    pub fn build(mut self) -> Self {
        let source = self.source_mode();
        let config = format!("{}-{}-{}", &self.mode, source, &self.codec);
        match self.store.as_mut() {
            Some(store) => {
                store.config = Some(config);
            }
            None => {
                self.store = Some(StoreSettings {
                    config: Some(config),
                    store_type: None,
                    snapshot: None,
                });
            }
        }
        self.source = Some(source);
        self
    }

    /// Sets the age of the index in milliseconds to the given epoch time
    pub fn age(self, epoch_millis: u64) -> Self {
        Self {
            age: self.creation_date.map(|date| epoch_millis - date),
            ..self
        }
    }

    /// Sets the data stream for the index
    pub fn data_stream(self, data_stream: Option<DataStream>) -> Self {
        let is_data_stream_write_index = data_stream.as_ref().map_or(false, |ds| ds.is_write_index);
        Self {
            data_stream,
            is_write_index: self.is_write_index || is_data_stream_write_index,
            ..self
        }
    }

    /// Adds the name of the index
    pub fn name(self, name: String) -> Self {
        Self {
            name: Some(name),
            ..self
        }
    }

    /// Returns the mapping.source.mode
    fn source_mode(&self) -> String {
        self.mapping
            .as_ref()
            .and_then(|mapping_settings| mapping_settings.as_object())
            .and_then(|mapping| mapping.get("source"))
            .and_then(|source| source.get("mode"))
            .and_then(|mode| mode.as_str())
            .unwrap_or("default")
            .to_string()
    }

    /// Returns select lifecycle fields (name, rollover_alias and indexing_complete)
    pub fn get_lifecycle(&self) -> Value {
        json!({
            "name": self.lifecycle.as_ref().and_then(|lifecycle| lifecycle.get("name")),
            "rollover_alias": self.lifecycle.as_ref().and_then(|lifecycle| lifecycle.get("rollover_alias")),
            "indexing_complete": self.lifecycle.as_ref().and_then(|lifecycle| lifecycle.get("indexing_complete")),
        })
    }
}

impl std::default::Default for IndexSettings {
    fn default() -> Self {
        IndexSettings {
            allocation: None,
            auto_expand_replicas: None,
            blocks: None,
            codec: "unknown".to_string(),
            creation_date: None,
            default_pipeline: None,
            final_pipeline: None,
            hidden: None,
            is_write_index: false,
            lifecycle: None,
            mapping: None,
            mode: "unkown".to_string(),
            number_of_replicas: None,
            number_of_shards: None,
            priority: None,
            provided_name: None,
            query: None,
            refresh_interval: "unkown".to_string(),
            routing: None,
            shard: None,
            shard_limit: None,
            source: None,
            store: None,
            sort: None,
            uuid: "".to_string(),
            version: Value::Null,
            age: None,
            data_stream: None,
            name: None,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Settings {
    pub settings: Index,
}

#[derive(Deserialize, Serialize)]
pub struct Index {
    pub index: IndexSettings,
}

fn default_to_default() -> String {
    String::from("default")
}

fn default_to_standard() -> String {
    String::from("standard")
}

fn deserialize_codec<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<Value> = Deserialize::deserialize(deserializer)?;

    match value {
        Some(Value::String(s)) => Ok(s),
        Some(Value::Null) => Ok(default_to_default()),
        Some(_) => Err(serde::de::Error::custom("codec expects a string or null")),
        None => Ok(default_to_default()),
    }
}

/// The standard deserializer from serde_json does not deserializing numbers from
/// strings. Unfortunately the _settings API frequently wraps numbers in quotes.

fn number_from_string<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;

    match value {
        Value::Number(num) => Ok(num.as_u64()),
        Value::String(s) => Ok(s.parse::<u64>().ok()),
        Value::Null => Ok(None),
        _ => Err(serde::de::Error::custom(
            "expected a number or a string representing a number",
        )),
    }
}

impl DataSource for IndicesSettings {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("settings.json"),
            PathType::Url => Ok("_all/_settings"),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::IndicesSettings)
    }
}
