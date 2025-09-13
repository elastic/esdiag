// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{exporter::Exporter, processor::ProcessorSummary};

use super::{
    super::{DocumentExporter, ElasticsearchMetadata, Lookups, Metadata},
    IlmPolicies, IlmPolicy,
};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;

impl DocumentExporter<Lookups, ElasticsearchMetadata> for IlmPolicies {
    async fn documents_export(
        self,
        exporter: &Exporter,
        _lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        log::debug!("processing ILM policies");
        let data_stream = "settings-ilm-esdiag".to_string();
        let metadata = metadata.for_data_stream(&data_stream).as_meta_doc();

        let mut policies: Vec<(String, IlmPolicy)> = self.into_par_iter().collect();

        let policies: Vec<Value> = policies
            .par_drain(..)
            .filter_map(|(name, config)| {
                serde_json::to_value(IlmDoc {
                    ilm: IlmPolicyDoc { name, config },
                    metadata: metadata.clone(),
                })
                .ok()
            })
            .collect();

        log::debug!("ILM policies docs: {}", policies.len());
        let mut summary = ProcessorSummary::new("settings-ilm-policies-esdiag".to_string());
        match exporter
            .send("settings-ilm-policies-esdiag".to_string(), policies)
            .await
        {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => log::error!("Failed to send ILM policies: {}", err),
        }
        summary
    }
}

#[derive(Serialize)]
struct IlmDoc {
    ilm: IlmPolicyDoc,
    #[serde(flatten)]
    metadata: Value,
}

#[derive(Serialize)]
struct IlmPolicyDoc {
    name: String,
    #[serde(flatten)]
    config: IlmPolicy,
}
