// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::diagnostic::{DataStreamName, DiagnosticManifest, DiagnosticMetadata};
use super::{Metadata, version::Version};
use eyre::Result;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Serialize)]
pub struct LogstashMetadata {
    pub node: Version,
    pub diagnostic: DiagnosticMetadata,
    pub timestamp: u64,
    pub as_doc: MetadataDoc,
}

#[derive(Clone, Serialize)]
pub struct MetadataDoc {
    #[serde(rename = "@timestamp")]
    pub timestamp: u64,
    pub node: Version,
    pub diagnostic: DiagnosticMetadata,
    pub data_stream: DataStreamName,
}

impl Metadata for MetadataDoc {
    fn as_meta_doc(&self) -> Value {
        serde_json::to_value(self).expect("Failed to serialize metadata")
    }
}

impl LogstashMetadata {
    pub fn try_new(manifest: DiagnosticManifest, node: Version) -> Result<Self> {
        let name = node.name.replace(" ", "_");
        let diagnostic = DiagnosticMetadata::try_from(manifest.with_name(name))?;
        let timestamp = diagnostic.collection_date;

        let as_doc = MetadataDoc {
            timestamp,
            node: node.clone(),
            diagnostic: diagnostic.clone(),
            data_stream: DataStreamName::from("metrics-default-esdiag"),
        };

        Ok(Self {
            as_doc,
            node,
            diagnostic,
            timestamp,
        })
    }

    pub fn for_data_stream(&self, data_stream: &str) -> MetadataDoc {
        MetadataDoc {
            data_stream: DataStreamName::from(data_stream),
            ..self.as_doc.clone()
        }
    }
}
