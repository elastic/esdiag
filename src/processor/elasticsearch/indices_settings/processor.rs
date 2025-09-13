// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{exporter::Exporter, processor::ProcessorSummary};

use super::{
    super::{DocumentExporter, ElasticsearchMetadata, Lookups, Metadata},
    IndexSettings, IndicesSettings,
};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;

impl DocumentExporter<Lookups, ElasticsearchMetadata> for IndicesSettings {
    async fn documents_export(
        mut self,
        exporter: &Exporter,
        lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        log::debug!("processing indices: {}", self.len());
        let index_metadata = metadata.for_data_stream("settings-index-esdiag");
        let collection_date = metadata.timestamp;
        let metadata_doc = index_metadata.as_meta_doc();

        let index_settings: Vec<EnrichedIndexSettings> = self
            .par_drain()
            .map(|(name, settings)| {
                let index_settings = settings
                    .settings
                    .index
                    .data_stream(lookups.data_stream.by_id(&name).cloned())
                    .age(collection_date)
                    .name(name)
                    .build();

                EnrichedIndexSettings {
                    index: index_settings,
                    metadata: metadata_doc.clone(),
                }
            })
            .collect();

        log::debug!("index settings docs: {}", index_settings.len());
        let mut summary = ProcessorSummary::new(index_metadata.data_stream.to_string());
        match exporter
            .send(index_metadata.data_stream.to_string(), index_settings)
            .await
        {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => log::error!("Failed to send index settings: {}", err),
        }
        summary
    }
}

#[derive(Clone, Serialize)]
struct EnrichedIndexSettings {
    index: IndexSettings,
    #[serde(flatten)]
    metadata: Value,
}
