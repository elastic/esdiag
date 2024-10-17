use super::{DataProcessor, ElasticsearchMetadata, Lookups};
use crate::{
    data::elasticsearch::{DataStream, IndexSettings, IndicesSettings},
    processor::Metadata,
};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

impl DataProcessor<ElasticsearchMetadata> for IndicesSettings {
    fn generate_docs(
        mut self,
        lookups: Arc<Lookups>,
        metadata: Arc<ElasticsearchMetadata>,
    ) -> (String, Vec<Value>) {
        log::debug!("processing indices: {}", self.len());
        let data_stream = "settings-index-esdiag".to_string();
        let index_metadata = metadata.for_data_stream(&data_stream).as_meta_doc();
        let collection_date = metadata.timestamp;
        let data_stream_lookup = lookups.data_stream.clone();

        let index_settings: Vec<Value> = self
            .par_drain()
            .filter_map(|(name, settings)| {
                let index = settings.index();
                let creation_date = index.creation_date.expect("creation_date not found");
                let age = collection_date - creation_date;
                let data_stream = data_stream_lookup.by_id(&name).cloned();
                let index_settings_doc = IndexSettingsDoc::from(index).with(
                    name,
                    age,
                    data_stream,
                    index_metadata.clone(),
                );

                serde_json::to_value(index_settings_doc).ok()
            })
            .collect();

        log::debug!("index setting docs: {}", index_settings.len());
        (data_stream, index_settings)
    }
}

#[derive(Clone, Serialize)]
struct IndexSettingsDoc {
    #[serde(flatten)]
    metadata: Option<Value>,
    index: Option<IndexSettings>,
}

impl IndexSettingsDoc {
    fn with(
        self,
        name: String,
        age: i64,
        data_stream: Option<DataStream>,
        metadata: Value,
    ) -> Self {
        let index = self.index.map(|mut index| {
            index.age = Some(age);
            index.data_stream = data_stream;
            index.name = Some(name);
            index
        });

        Self {
            metadata: Some(metadata),
            index,
        }
    }
}

impl From<IndexSettings> for IndexSettingsDoc {
    fn from(index: IndexSettings) -> Self {
        IndexSettingsDoc {
            metadata: None,
            index: Some(index),
        }
    }
}
