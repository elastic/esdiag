use super::{
    super::{DataProcessor, ElasticsearchMetadata, Lookups, Metadata},
    IndexSettings, IndicesSettings,
};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

impl DataProcessor<Lookups, ElasticsearchMetadata> for IndicesSettings {
    fn generate_docs(
        mut self,
        lookups: Arc<Lookups>,
        metadata: Arc<ElasticsearchMetadata>,
    ) -> (String, Vec<Value>) {
        log::debug!("processing indices: {}", self.len());
        let index_metadata = metadata.for_data_stream("settings-index-esdiag");
        let collection_date = metadata.timestamp;

        let index_settings: Vec<Value> = self
            .par_drain()
            .filter_map(|(name, settings)| {
                let index_settings = settings
                    .settings
                    .index
                    .data_stream(lookups.data_stream.by_id(&name).cloned())
                    .age(collection_date)
                    .name(name)
                    .build();

                serde_json::to_value(EnrichedIndexSettings {
                    index: index_settings,
                    metadata: index_metadata.as_meta_doc(),
                })
                .ok()
            })
            .collect();

        log::debug!("index setting docs: {}", index_settings.len());
        (index_metadata.data_stream.to_string(), index_settings)
    }
}

#[derive(Clone, Serialize)]
struct EnrichedIndexSettings {
    index: IndexSettings,
    #[serde(flatten)]
    metadata: Value,
}
