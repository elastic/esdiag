// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, KibanaMetadata, Lookups, Metadata, send_documents};
use super::Spaces;
use crate::{exporter::Exporter, processor::ProcessorSummary};
use serde::Serialize;
use serde_json::{Value, json};

impl DocumentExporter<Lookups, KibanaMetadata> for Spaces {
    async fn documents_export(self, exporter: &Exporter, _: &Lookups, metadata: &KibanaMetadata) -> ProcessorSummary {
        let data_stream = "settings-kibana.spaces-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let docs = process_spaces_data(self.0, metadata_doc);

        send_documents(exporter, &data_stream, docs).await
    }
}

#[derive(Serialize)]
struct SpaceDoc {
    #[serde(flatten)]
    metadata: Value,
    #[serde(flatten)]
    data: Value,
}

fn process_spaces_data(data: Value, metadata: Value) -> Vec<Value> {
    let items = match data {
        Value::Array(arr) => arr,
        _ => vec![data],
    };

    items
        .into_iter()
        .map(|item| {
            json!(SpaceDoc {
                metadata: metadata.clone(),
                data: item
            })
        })
        .collect()
}
