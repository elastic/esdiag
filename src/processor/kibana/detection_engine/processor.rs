// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, KibanaMetadata, Lookups, Metadata, send_documents};
use super::{DetectionEngineHealth, DetectionEngineRules};
use crate::{exporter::Exporter, processor::ProcessorSummary};
use serde::Serialize;
use serde_json::{Value, json};

impl DocumentExporter<Lookups, KibanaMetadata> for DetectionEngineHealth {
    async fn documents_export(self, exporter: &Exporter, _: &Lookups, metadata: &KibanaMetadata) -> ProcessorSummary {
        let data_stream = "metrics-kibana.detection_engine-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let doc = json!(DetectionEngineDoc {
            metadata: metadata_doc,
            data: self.0
        });

        send_documents(exporter, &data_stream, vec![doc]).await
    }
}

impl DocumentExporter<Lookups, KibanaMetadata> for DetectionEngineRules {
    async fn documents_export(self, exporter: &Exporter, _: &Lookups, metadata: &KibanaMetadata) -> ProcessorSummary {
        let data_stream = "settings-kibana.detection_engine-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let docs = process_rules_data(self.0, metadata_doc);

        send_documents(exporter, &data_stream, docs).await
    }
}

#[derive(Serialize)]
struct DetectionEngineDoc {
    #[serde(flatten)]
    metadata: Value,
    #[serde(flatten)]
    data: Value,
}

fn process_rules_data(data: Value, metadata: Value) -> Vec<Value> {
    let items = if let Some(data) = data.get("data").and_then(|d| d.as_array()) {
        data.clone()
    } else if let Some(arr) = data.as_array() {
        arr.clone()
    } else {
        vec![data]
    };

    items
        .into_iter()
        .map(|item| {
            json!(DetectionEngineDoc {
                metadata: metadata.clone(),
                data: item
            })
        })
        .collect()
}
