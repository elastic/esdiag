// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    super::{DataProcessor, ElasticsearchMetadata, Lookups, Metadata},
    SlmPolicies, SlmPolicy,
};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;

impl DataProcessor<Lookups, ElasticsearchMetadata> for SlmPolicies {
    fn generate_docs(
        self,
        _lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> (String, Vec<Value>) {
        log::debug!("processing SLM policies");
        let data_stream = "settings-slm-esdiag".to_string();
        let metadata = metadata.for_data_stream(&data_stream).as_meta_doc();

        let mut policies: Vec<(String, SlmPolicy)> = self.into_par_iter().collect();

        let policies: Vec<Value> = policies
            .par_drain(..)
            .filter_map(|(name, config)| {
                serde_json::to_value(SlmDoc {
                    slm: SlmPolicyDoc { name, config },
                    metadata: metadata.clone(),
                })
                .ok()
            })
            .collect();

        log::debug!("slm policy docs: {}", policies.len());
        (data_stream, policies)
    }
}

#[derive(Serialize)]
struct SlmDoc {
    slm: SlmPolicyDoc,
    #[serde(flatten)]
    metadata: Value,
}

#[derive(Serialize)]
struct SlmPolicyDoc {
    name: String,
    #[serde(flatten)]
    config: SlmPolicy,
}
