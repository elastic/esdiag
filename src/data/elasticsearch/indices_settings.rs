use super::DataStream;
use crate::data::diagnostic::{data_source::PathType, elasticsearch::DataSet, DataSource};
use color_eyre::eyre::Result;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

pub type IndicesSettings = HashMap<String, Settings>;

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
    pub store: Option<Value>,
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

impl IndexSettings {
    /// Determines additional field values from previously deserialized data
    pub fn build(mut self) -> Self {
        let source = self.source_mode();
        let config = json!({"config": format!("{}-{}-{}", &self.mode, source, &self.codec)});
        self.source = Some(source);
        match self.store {
            Some(ref mut store) => json_patch::merge(store, &config),
            None => self.store = Some(config),
        };
        self
    }

    /// Sets the age of the index in milliseconds to the given epoch time
    pub fn age(mut self, epoch_millis: u64) -> Self {
        self.age = self.creation_date.map(|date| epoch_millis - date);
        self
    }

    /// Sets the data stream for the index
    pub fn data_stream(mut self, data_stream: Option<DataStream>) -> Self {
        self.data_stream = data_stream;
        self
    }

    /// Adds the name of the index
    pub fn name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    /// Returns the concatinated string of mode, mapping.source.mode and codec
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
            codec: "unkown".to_string(),
            creation_date: None,
            default_pipeline: None,
            final_pipeline: None,
            hidden: None,
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
    settings: Index,
}

impl Settings {
    /// Consume `self` and return the index settings, dropping the unnecessary parent data.
    pub fn index(self) -> IndexSettings {
        self.settings.index
    }
}

#[derive(Deserialize, Serialize)]
struct Index {
    index: IndexSettings,
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
