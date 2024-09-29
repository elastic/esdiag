use super::lookup::index::IndexData;
use super::metadata::{DataStreamName, Metadata, MetadataDoc};
use crate::data::elasticsearch::{IndexSettings, IndicesSettings};
use serde::Serialize;
use serde_json::{json, Value};

pub fn enrich_lookup(metadata: &mut Metadata, data: String) -> Vec<Value> {
    let lookup = &mut metadata.lookup;
    let metadata = &metadata.as_doc;
    let indices_settings: IndicesSettings = match serde_json::from_str(&data) {
        Ok(data) => data,
        Err(e) => {
            log::error!("Failed to deserialize index_settings: {}", e);
            return Vec::<Value>::new();
        }
    };

    log::debug!("indices: {}", indices_settings.len());

    let index_settings_doc = IndexSettingsDoc::new(
        metadata.clone(),
        DataStreamName::from("settings-index-esdiag"),
    );

    let index_settings: Vec<Value> = indices_settings
        .into_iter()
        .map(|(name, settings)| {
            let index = settings.index();
            let creation_date = index.creation_date.expect("creation_date not found");
            let age = metadata.timestamp - creation_date;
            let indexing_complete = match &index.lifecycle {
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
                codec: index.codec.clone(),
                creation_date: index.creation_date,
                hidden: index.hidden.clone(),
                indexing_complete,
                refresh_interval: index.refresh_interval.clone(),
            };
            lookup.index_settings.add(index_data).with_name(&name);

            let mut index_settings_doc = index_settings_doc.clone().with(index);
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
    data_stream: DataStreamName,
    index: Option<IndexSettings>,
}

impl IndexSettingsDoc {
    pub fn new(metadata: MetadataDoc, data_stream: DataStreamName) -> Self {
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
