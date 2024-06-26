use super::lookup::data_stream::DataStreamDoc;
use super::lookup::index::IndexData;
use super::metadata::{DataStream, Metadata, MetadataDoc};
use serde::{self, Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn enrich_lookup(metadata: &mut Metadata, data: String) -> Vec<Value> {
    let lookup = &mut metadata.lookup;
    let metadata = &metadata.as_doc;
    let indices: HashMap<String, Settings> = match serde_json::from_str(&data) {
        Ok(data) => data,
        Err(e) => {
            log::error!("Failed to deserialize index_settings: {}", e);
            return Vec::<Value>::new();
        }
    };

    log::debug!("indices: {}", indices.len());

    let index_settings_doc =
        IndexSettingsDoc::new(metadata.clone(), DataStream::from("settings-index-esdiag"));

    let index_settings: Vec<Value> = indices
        .into_iter()
        .map(|(name, settings)| {
            let settings = settings.settings.index;
            let creation_date = settings.creation_date.expect("creation_date not found");
            let age = metadata.timestamp - creation_date;
            let indexing_complete = match &settings.lifecycle {
                Some(l) => match l.get("indexing_complete") {
                    Some(Value::String(s)) => match s.as_str() {
                        "true" => Some(true),
                        _ => Some(false),
                    },
                    _ => None,
                },
                None => None,
            };

            let index_data = IndexData {
                age: Some(age),
                creation_date: settings.creation_date,
                indexing_complete,
            };
            lookup.index.add(index_data).with_name(&name);

            let mut index_settings_doc = index_settings_doc.clone().with(settings);
            index_settings_doc.index.as_mut().map(|index| {
                index.age = Some(age);
                index.data_stream = lookup.data_stream.by_name(&name).cloned();
                index.name = Some(name);
            });

            json!(index_settings_doc)
        })
        .collect();

    log::debug!("index setting docs: {}", index_settings.len());
    index_settings
}

// Serializing data structures

#[derive(Clone, Serialize)]
pub struct IndexSettingsDoc {
    #[serde(flatten)]
    metadata: MetadataDoc,
    data_stream: DataStream,
    index: Option<IndexSettings>,
}

impl IndexSettingsDoc {
    pub fn new(metadata: MetadataDoc, data_stream: DataStream) -> Self {
        IndexSettingsDoc {
            data_stream,
            index: None,
            metadata,
        }
    }
    pub fn with(mut self, settings: IndexSettings) -> Self {
        self.index = Some(settings);
        self
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct IndexSettings {
    allocation: Option<Value>,
    auto_expand_replicas: Option<String>,
    blocks: Option<Value>,
    codec: Option<String>,
    #[serde(deserialize_with = "number_from_string")]
    creation_date: Option<i64>,
    default_pipeline: Option<String>,
    final_pipeline: Option<String>,
    hidden: Option<String>,
    lifecycle: Option<Value>,
    mapping: Option<Value>,
    #[serde(deserialize_with = "number_from_string")]
    number_of_replicas: Option<i64>,
    #[serde(deserialize_with = "number_from_string")]
    number_of_shards: Option<i64>,
    priority: Option<String>,
    provided_name: String,
    query: Option<Value>,
    refresh_interval: Option<String>,
    routing: Option<Value>,
    shard: Option<Value>,
    shard_limit: Option<Value>,
    store: Option<Value>,
    sort: Option<Value>,
    uuid: String,
    version: Value,
    // Not in source json
    #[serde(skip_deserializing)]
    age: Option<i64>,
    #[serde(skip_deserializing)]
    data_stream: Option<DataStreamDoc>,
    #[serde(skip_deserializing)]
    name: Option<String>,
}

// Deserializing data structures

#[derive(Deserialize)]
struct Settings {
    settings: Index,
}

#[derive(Deserialize)]
struct Index {
    index: IndexSettings,
}

// The standard deserializer from serde_json does not deserializing numbers from
// strings. Unfortunately the _settings API frequently wraps numbers in quotes.

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
