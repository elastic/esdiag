use super::lookup::index::IndexData;
use super::metadata::Metadata;
use crate::processor::elasticsearch::lookup::Identifiers;
use json_patch::merge;
use serde::{self, Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

fn number_from_string<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;

    match value {
        Value::Number(num) => num
            .as_i64()
            .ok_or_else(|| serde::de::Error::custom("expected a number")),
        Value::String(s) => s
            .parse::<i64>()
            .map_err(|_| serde::de::Error::custom("expected a string representing a number")),
        _ => Err(serde::de::Error::custom(
            "expected a number or a string representing a number",
        )),
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Settings {
    settings: Index,
}

#[derive(Debug, Serialize, Deserialize)]
struct Index {
    index: IndexSettings,
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexSettings {
    mapping: Option<Value>,
    hidden: Option<String>,
    provided_name: String,
    final_pipeline: Option<String>,
    query: Option<Value>,
    #[serde(deserialize_with = "number_from_string")]
    creation_date: i64,
    sort: Option<Value>,
    priority: Option<String>,
    #[serde(deserialize_with = "number_from_string")]
    number_of_replicas: i64,
    uuid: String,
    version: Value,
    lifecycle: Option<Value>,
    codec: Option<String>,
    routing: Option<Value>,
    #[serde(deserialize_with = "number_from_string")]
    number_of_shards: i64,
    default_pipeline: Option<String>,
}

pub fn enrich_lookup(metadata: &mut Metadata, data: String) -> Vec<Value> {
    let indices: HashMap<String, Settings> = match serde_json::from_str(&data) {
        Ok(data) => data,
        Err(e) => {
            log::error!("Failed to deserialize index_settings: {}", e);
            return Vec::<Value>::new();
        }
    };

    log::debug!("indices: {}", indices.len());

    let data_stream = json!({
        "data_stream": {
            "dataset": "index",
            "namespace": "esdiag",
            "type": "settings",
        }
    });
    let mut index_settings = Vec::new();
    for (index, settings) in indices {
        let creation_date = settings.settings.index.creation_date;
        let age = metadata.diagnostic.collection_date - creation_date;
        let indexing_complete = match &settings.settings.index.lifecycle {
            Some(l) => match l.get("indexing_complete") {
                Some(Value::String(s)) => match s.as_str() {
                    "true" => Some(true),
                    _ => Some(false),
                },
                _ => None,
            },
            None => None,
        };
        metadata.lookup.index.insert(
            Identifiers {
                id: None,
                name: Some(index.clone()),
                host: None,
                ip: None,
            },
            IndexData {
                indexing_complete,
                creation_date: Some(creation_date),
            },
        );

        let mut doc = json!({
            "@timestamp": metadata.diagnostic.collection_date,
            "cluster": metadata.cluster,
            "diagnostic": metadata.diagnostic,
            "index": settings.settings.index,
        });
        let doc_patch = json!({
            "index": {
                "age": age,
                "name": index,
                "data_stream": metadata.lookup.data_stream.by_name(&index),
            },
        });
        merge(&mut doc, &doc_patch);
        merge(&mut doc, &data_stream);
        index_settings.push(doc);
    }

    log::debug!("index setting docs: {}", index_settings.len());
    index_settings
}
