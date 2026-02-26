// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::diagnostic::{DataStreamName, DiagnosticManifest, DiagnosticMetadata};
use super::version::{Cluster, ClusterMetadata};
use super::Metadata;
use eyre::Result;
use serde::{Deserialize, Serialize, Serializer, Deserializer};
use serde_json::value::RawValue;
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
        let raw = serde_json::value::to_raw_value(&temp).expect("Failed to serialize metadata");
        MetadataRawValue(Arc::from(raw))
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
        let raw = serde_json::value::to_raw_value(&temp).expect("Failed to serialize metadata");
        let as_doc = MetadataRawValue(Arc::from(raw));

        Ok(Self {
            as_doc,
            cluster,
            diagnostic,
            timestamp,
        })
    }
}

#[derive(Clone)]
pub struct MetadataRawValue(pub Arc<RawValue>);

impl Serialize for MetadataRawValue {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // MetadataRawValue is always flattened, so we want to emit the fields of the inner JSON object.
        let value: serde_json::Value =
            serde_json::from_str(self.0.get()).map_err(serde::ser::Error::custom)?;
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MetadataRawValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // For testing we need to be able to deserialize flattened values back into a string,
        // although this doesn't fully reconstruct the original string, we don't care about it
        // during tests
        let value: serde_json::Value = Deserialize::deserialize(deserializer)?;
        let raw = serde_json::value::to_raw_value(&value).map_err(serde::de::Error::custom)?;
        Ok(MetadataRawValue(Arc::from(raw)))
    }
}

impl MetadataRawValue {
    pub fn as_meta_doc(&self) -> serde_json::Value {
        serde_json::from_str(self.0.get()).expect("Failed to parse metadata raw value")
    }
}

impl Metadata for MetadataRawValue {
    fn as_meta_doc(&self) -> serde_json::Value {
        self.as_meta_doc()
    }
}
