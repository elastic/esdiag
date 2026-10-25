// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, KibanaMetadata, Lookups, Metadata, document, documents, send_documents};
use super::{AgentPolicies, AgentStatus, Agents, Packages};
use crate::{exporter::Exporter, processor::ProcessorSummary};

/// Fleet list responses wrap items in `list` or `items`; Kibana 7.x packages use `response`.
const FLEET_LIST_KEYS: &[&str] = &["list", "items", "response"];

impl DocumentExporter<Lookups, KibanaMetadata> for Agents {
    async fn documents_export(self, exporter: &Exporter, _: &Lookups, metadata: &KibanaMetadata) -> ProcessorSummary {
        let data_stream = "settings-kibana.fleet-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let docs = documents(&metadata_doc, self.0, FLEET_LIST_KEYS);

        send_documents(exporter, &data_stream, docs).await
    }
}

impl DocumentExporter<Lookups, KibanaMetadata> for AgentPolicies {
    async fn documents_export(self, exporter: &Exporter, _: &Lookups, metadata: &KibanaMetadata) -> ProcessorSummary {
        let data_stream = "settings-kibana.fleet-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let docs = documents(&metadata_doc, self.0, FLEET_LIST_KEYS);

        send_documents(exporter, &data_stream, docs).await
    }
}

impl DocumentExporter<Lookups, KibanaMetadata> for Packages {
    async fn documents_export(self, exporter: &Exporter, _: &Lookups, metadata: &KibanaMetadata) -> ProcessorSummary {
        let data_stream = "settings-kibana.fleet-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let docs = documents(&metadata_doc, self.0, FLEET_LIST_KEYS);

        send_documents(exporter, &data_stream, docs).await
    }
}

impl DocumentExporter<Lookups, KibanaMetadata> for AgentStatus {
    async fn documents_export(self, exporter: &Exporter, _: &Lookups, metadata: &KibanaMetadata) -> ProcessorSummary {
        let data_stream = "metrics-kibana.fleet-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let doc = document(&metadata_doc, self.0);

        send_documents(exporter, &data_stream, vec![doc]).await
    }
}
