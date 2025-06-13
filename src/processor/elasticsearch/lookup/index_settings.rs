use crate::data::{
    diagnostic::Lookup,
    elasticsearch::{DataStream, IlmStats, IndexSettings, IndicesSettings, StoreSettings},
};
use eyre::Result;
use serde::Serialize;
use serde_with::skip_serializing_none;

impl From<IndicesSettings> for Lookup<IndexSettings> {
    fn from(mut indices_settings: IndicesSettings) -> Self {
        let mut lookup = Lookup::<IndexSettings>::new();
        indices_settings.drain().for_each(|(name, settings)| {
            let index_settings = settings.settings.index.name(name.clone()).build();
            let id = index_settings.uuid.clone();
            lookup.add(index_settings).with_name(&name).with_id(&id);
        });
        lookup
    }
}

impl From<Result<IndicesSettings>> for Lookup<IndexSettings> {
    fn from(indices_settings: Result<IndicesSettings>) -> Self {
        match indices_settings {
            Ok(indices_settings) => Lookup::<IndexSettings>::from(indices_settings),
            Err(e) => {
                log::warn!("Failed to parse IndicesSettings: {}", e);
                Lookup::new()
            }
        }
    }
}

#[skip_serializing_none]
#[derive(Clone, Serialize)]
pub struct IndexSettingsDocument {
    pub age: Option<u64>,
    pub codec: String,
    pub creation_date: u64,
    pub data_stream: Option<DataStream>,
    pub lifecycle: Option<serde_json::Value>,
    pub ilm: Option<IlmStats>,
    pub is_write_index: bool,
    pub mode: String,
    pub name: String,
    pub store: Option<StoreSettings>,
    pub number_of_shards: Option<u64>,
    pub number_of_replicas: Option<u64>,
    pub refresh_interval: String,
    pub source: Option<String>,
    pub write_phase_sec: Option<u64>,
    pub uuid: String,
}

impl IndexSettingsDocument {
    pub fn ilm(self, ilm: Option<IlmStats>) -> Self {
        Self { ilm, ..self }
    }

    pub fn write_phase(self, write_phase_sec: Option<u64>) -> Self {
        Self {
            write_phase_sec,
            ..self
        }
    }
}

impl From<IndexSettings> for IndexSettingsDocument {
    fn from(index_settings: IndexSettings) -> Self {
        log::trace!(
            "Datastream for {:?}: {:?}",
            index_settings.name,
            index_settings.data_stream
        );
        IndexSettingsDocument {
            age: index_settings.age,
            codec: index_settings.codec,
            creation_date: index_settings.creation_date.unwrap_or(0),
            data_stream: index_settings.data_stream,
            lifecycle: index_settings.lifecycle,
            ilm: None,
            is_write_index: index_settings.is_write_index,
            mode: index_settings.mode,
            name: index_settings.name.expect("Name is required"),
            store: index_settings.store,
            number_of_shards: index_settings.number_of_shards,
            number_of_replicas: index_settings.number_of_replicas,
            refresh_interval: index_settings.refresh_interval,
            source: index_settings.source,
            write_phase_sec: None,
            uuid: index_settings.uuid,
        }
    }
}
