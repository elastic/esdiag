use super::lookup::index::IndexData;
use super::metadata::Metadata;
use chrono::DateTime;
use json_patch::merge;
use serde_json::{json, Value};

pub async fn enrich_lookup(metadata: &mut Metadata, data: Value) -> Vec<Value> {
    let indices: Vec<_> = match data.as_object() {
        Some(data) => data.into_iter().collect(),
        None => return Vec::<Value>::new(),
    };
    log::debug!("indices: {}", indices.len());

    let data_stream = json!({
        "data_stream": {
            "dataset": "index",
            "namespace": "esdiag",
            "type": "settings",
        }
    });

    let collection_date = DateTime::parse_from_rfc3339(&metadata.diagnostic.collection_date)
        .expect("Failed to parse collection_date")
        .timestamp_millis();

    let mut index_settings = Vec::new();

    for (index, settings) in &indices {
        let creation_date = match settings["settings"]["index"]["creation_date"].as_str() {
            Some(date) => match date.parse::<i64>() {
                Ok(date) => date,
                Err(e) => {
                    log::warn!("Failed to parse creation_date: {}", e);
                    continue;
                }
            },
            None => {
                log::warn!(
                    "Failed to parse creation_date from value {}",
                    settings["settings"]["index"]["creation_date"]
                );
                continue;
            }
        };
        let age = collection_date - creation_date;

        metadata.lookup.index.insert(index, &json!({ "age": age }));

        let mut doc = json!({
            "@timestamp": metadata.diagnostic.collection_date,
            "cluster": metadata.cluster,
            "data_stream": metadata.lookup.data_stream.by_index(index.as_str()),
            "diagnostic": metadata.diagnostic,
            "index": settings["settings"]["index"],
        });
        let doc_patch = json!({
            "index": {
                "age": age,
                "name": index
            },
        });
        merge(&mut doc, &doc_patch);
        merge(&mut doc, &data_stream);
        index_settings.push(doc);
    }

    log::debug!("index setting docs: {}", index_settings.len());
    index_settings
}
