// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, KibanaMetadata, Lookups, Metadata, send_documents};
use super::{SyntheticsFilters, UptimeLocations};
use crate::{exporter::Exporter, processor::ProcessorSummary};
use serde::Serialize;
use serde_json::{Value, json};

impl DocumentExporter<Lookups, KibanaMetadata> for SyntheticsFilters {
    async fn documents_export(self, exporter: &Exporter, _: &Lookups, metadata: &KibanaMetadata) -> ProcessorSummary {
        let data_stream = "settings-kibana.synthetics-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let doc = json!(SyntheticsUptimeDoc {
            metadata: metadata_doc,
            data: self.0
        });

        send_documents(exporter, &data_stream, vec![doc]).await
    }
}

impl DocumentExporter<Lookups, KibanaMetadata> for UptimeLocations {
    async fn documents_export(self, exporter: &Exporter, _: &Lookups, metadata: &KibanaMetadata) -> ProcessorSummary {
        let data_stream = "settings-kibana.uptime-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let doc = json!(SyntheticsUptimeDoc {
            metadata: metadata_doc,
            data: self.0
        });

        send_documents(exporter, &data_stream, vec![doc]).await
    }
}

#[derive(Serialize)]
struct SyntheticsUptimeDoc {
    #[serde(flatten)]
    metadata: Value,
    #[serde(flatten)]
    data: Value,
}
