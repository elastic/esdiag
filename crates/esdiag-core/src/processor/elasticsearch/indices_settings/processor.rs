// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, ElasticsearchMetadata, Lookups, ProcessorSummary};
use super::{IndexSettings, IndicesSettings};
use crate::exporter::Exporter;
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
        tracing::debug!("processing indices: {}", self.len());
        let data_stream = "settings-index-esdiag";
        let index_metadata = metadata.for_data_stream(data_stream);
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

        tracing::debug!("index settings docs: {}", index_settings.len());
        let mut summary = ProcessorSummary::new(data_stream.to_string());
        match exporter.send(data_stream.to_string(), index_settings).await {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => tracing::error!("Failed to send index settings: {}", err),
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
