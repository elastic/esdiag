use super::{DataProcessor, ElasticsearchDiagnostic, Receiver};
use crate::{
    data::elasticsearch::{DataStream, IndexSettings, IndicesSettings},
    processor::Metadata,
};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

pub struct IndexSettingsProcessor {
    diagnostic: Arc<ElasticsearchDiagnostic>,
    receiver: Arc<Receiver>,
}

impl IndexSettingsProcessor {
    fn new(diagnostic: Arc<ElasticsearchDiagnostic>, receiver: Arc<Receiver>) -> Self {
        IndexSettingsProcessor {
            diagnostic,
            receiver,
        }
    }
}

impl From<Arc<ElasticsearchDiagnostic>> for IndexSettingsProcessor {
    fn from(diagnostic: Arc<ElasticsearchDiagnostic>) -> Self {
        IndexSettingsProcessor::new(diagnostic.clone(), diagnostic.receiver.clone())
    }
}

impl DataProcessor for IndexSettingsProcessor {
    async fn process(&self) -> (String, Vec<Value>) {
        let data_stream = "settings-index-esdiag".to_string();
        let index_metadata = self
            .diagnostic
            .metadata
            .clone()
            .for_data_stream(&data_stream)
            .as_meta_doc();
        let collection_date = self.diagnostic.metadata.timestamp;
        let data_stream_lookup = self.diagnostic.lookups.data_stream.clone();
        let mut indices_settings = match self.receiver.get::<IndicesSettings>().await {
            Ok(indices_settings) => {
                log::debug!("indices: {}", indices_settings.len());
                indices_settings
            }
            Err(e) => {
                log::error!("Failed to receive indices_settings: {}", e);
                return (data_stream, Vec::new());
            }
        };

        let index_settings: Vec<Value> = indices_settings
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
