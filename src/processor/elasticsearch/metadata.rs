// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::diagnostic::{DataStreamName, DiagnosticManifest, DiagnosticMetadata};
use super::version::{Cluster, ClusterMetadata};
use super::Metadata;
use eyre::Result;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::value::RawValue;

#[derive(Clone, Serialize)]
pub struct ElasticsearchMetadata {
    pub cluster: ClusterMetadata,
    pub diagnostic: DiagnosticMetadata,
    pub timestamp: u64,
    pub as_doc: MetadataDoc,
}

impl ElasticsearchMetadata {
    pub fn for_data_stream(&self, data_stream: &str) -> MetadataDoc {
        MetadataDoc {
            data_stream: DataStreamName::from(data_stream),
            ..self.as_doc.clone()
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct MetadataDoc {
    #[serde(rename = "@timestamp")]
    pub timestamp: u64,
    pub cluster: ClusterMetadata,
    pub diagnostic: DiagnosticMetadata,
    pub data_stream: DataStreamName,
}

impl MetadataDoc {
    pub fn as_raw_value(&self) -> Box<RawValue> {
        serde_json::value::to_raw_value(&self).expect("Failed to serialize metadata")
    }

    /// Creates a pre-serialized version of this metadata for high-performance reuse.
    pub fn pre_serialize(&self) -> PreSerializedMetadata {
        PreSerializedMetadata {
            raw: self.as_raw_value(),
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct PreSerializedMetadata {
    raw: Box<RawValue>,
}

impl Serialize for PreSerializedMetadata {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // When flattened, we want to emit the fields of the inner JSON object.
        let value: serde_json::Value =
            serde_json::from_str(self.raw.get()).map_err(serde::ser::Error::custom)?;
        value.serialize(serializer)
    }
}

impl Metadata for MetadataDoc {
    fn as_meta_doc(&self) -> serde_json::Value {
        serde_json::to_value(&self).expect("Failed to serialize metadata")
    }
}

impl ElasticsearchMetadata {
    pub fn try_new(manifest: DiagnosticManifest, cluster: Cluster) -> Result<Self> {
        let name = cluster.display_name.replace(" ", "_");
        let diagnostic = DiagnosticMetadata::try_from(manifest.with_name(name))?;
        let timestamp = diagnostic.collection_date;
        let cluster = ClusterMetadata::from(cluster);

        let as_doc = MetadataDoc {
            timestamp,
            cluster: cluster.clone(),
            diagnostic: diagnostic.clone(),
            data_stream: DataStreamName::from("metrics-default-esdiag"),
        };

        Ok(Self {
            as_doc,
            cluster,
            diagnostic,
            timestamp,
        })
    }
}
