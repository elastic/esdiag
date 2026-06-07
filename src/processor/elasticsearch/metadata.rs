// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::diagnostic::{DataStreamName, DiagnosticManifest, DiagnosticMetadata};
use super::Metadata;
use super::version::{Cluster, ClusterMetadata};
use eyre::Result;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

#[derive(Clone, Serialize)]
pub struct ElasticsearchMetadata {
    pub cluster: ClusterMetadata,
    pub diagnostic: DiagnosticMetadata,
    pub timestamp: u64,
    pub as_doc: MetadataRawValue,
}

#[derive(Serialize)]
struct MetadataDocTemp<'a> {
    #[serde(rename = "@timestamp")]
    pub timestamp: u64,
    pub cluster: &'a ClusterMetadata,
    pub diagnostic: &'a DiagnosticMetadata,
    pub data_stream: DataStreamName,
}

impl ElasticsearchMetadata {
    pub fn for_data_stream(&self, data_stream: &str) -> MetadataRawValue {
        let temp = MetadataDocTemp {
            timestamp: self.timestamp,
            cluster: &self.cluster,
            diagnostic: &self.diagnostic,
            data_stream: DataStreamName::from(data_stream),
        };
        let value: serde_json::Value = serde_json::to_value(&temp).expect("Failed to serialize metadata");
        MetadataRawValue(Arc::new(value))
    }

    pub fn try_new(manifest: DiagnosticManifest, cluster: Cluster) -> Result<Self> {
        let name = cluster.display_name.replace(" ", "_");
        let diagnostic = DiagnosticMetadata::try_from(manifest.with_name(name))?;
        let timestamp = diagnostic.collection_date;
        let cluster = ClusterMetadata::from(cluster);

        let temp = MetadataDocTemp {
            timestamp,
            cluster: &cluster,
            diagnostic: &diagnostic,
            data_stream: DataStreamName::from("metrics-default-esdiag"),
        };
        let value: serde_json::Value = serde_json::to_value(&temp).expect("Failed to serialize metadata");
        let as_doc = MetadataRawValue(Arc::new(value));

        Ok(Self {
            as_doc,
            cluster,
            diagnostic,
            timestamp,
        })
    }
}

// Cached metadata value for flattened serialization. Named RawValue for historical reasons;
// the inner type was changed from Arc<RawValue> to Arc<Value> to avoid per-serialize reparsing.
#[derive(Clone)]
pub struct MetadataRawValue(Arc<serde_json::Value>);

impl Serialize for MetadataRawValue {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MetadataRawValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: serde_json::Value = Deserialize::deserialize(deserializer)?;
        Ok(MetadataRawValue(Arc::new(value)))
    }
}

impl MetadataRawValue {
    pub fn as_meta_doc(&self) -> serde_json::Value {
        (*self.0).clone()
    }
}

impl Metadata for MetadataRawValue {
    fn as_meta_doc(&self) -> serde_json::Value {
        self.as_meta_doc()
    }
}
