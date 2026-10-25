// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, KibanaMetadata, Lookups, Metadata, document, send_documents};
use super::StackMonitoringHealth;
use crate::{exporter::Exporter, processor::ProcessorSummary};

impl DocumentExporter<Lookups, KibanaMetadata> for StackMonitoringHealth {
    async fn documents_export(self, exporter: &Exporter, _: &Lookups, metadata: &KibanaMetadata) -> ProcessorSummary {
        let data_stream = "metrics-kibana.stack_monitoring-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let doc = document(&metadata_doc, self.0);

        send_documents(exporter, &data_stream, vec![doc]).await
    }
}
