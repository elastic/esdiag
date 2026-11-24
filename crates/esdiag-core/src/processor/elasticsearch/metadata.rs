// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::diagnostic::{DataStreamName, DiagnosticManifest, DiagnosticMetadata};
use super::Metadata;
use super::version::{Cluster, ClusterMetadata};
use crate::data::{Application, Platform};
use eyre::Result;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Serialize)]
pub struct ElasticsearchMetadata {
    pub cluster: ClusterMetadata,
    pub diagnostic: DiagnosticMetadata,
    pub platform: Platform,
    pub application: Option<Application>,
    pub timestamp: u64,
    pub as_doc: MetadataValue,
}

#[derive(Serialize)]
struct MetadataDocTemp<'a> {
    #[serde(rename = "@timestamp")]
    pub timestamp: u64,
    pub cluster: &'a ClusterMetadata,
    pub diagnostic: DiagnosticDocMetadata<'a>,
    pub data_stream: DataStreamName,
}

/// The per-document `diagnostic.*` envelope: the diagnostic metadata plus the
/// platform/application classification of ADR-0001.
#[derive(Serialize)]
struct DiagnosticDocMetadata<'a> {
    #[serde(flatten)]
    metadata: &'a DiagnosticMetadata,
    platform: Platform,
    #[serde(skip_serializing_if = "Option::is_none")]
    application: Option<Application>,
}

impl ElasticsearchMetadata {
    pub fn for_data_stream(&self, data_stream: &str) -> MetadataValue {
        self.for_data_stream_at(data_stream, self.timestamp)
    }

    /// The envelope for a document whose `@timestamp` is its own sample time,
    /// not the collection date; `diagnostic.collection_date` is unchanged.
    pub fn for_data_stream_at(&self, data_stream: &str, timestamp: u64) -> MetadataValue {
        let temp = MetadataDocTemp {
            timestamp,
            cluster: &self.cluster,
            diagnostic: DiagnosticDocMetadata {
                metadata: &self.diagnostic,
                platform: self.platform,
                application: self.application,
            },
            data_stream: DataStreamName::from(data_stream),
        };
        let value = serde_json::to_value(&temp).expect("Failed to serialize metadata");
        MetadataValue(Arc::new(value))
    }

    pub fn try_new(manifest: DiagnosticManifest, cluster: Cluster) -> Result<Self> {
        let name = cluster.display_name.replace(" ", "_");
        let platform = manifest.platform();
        let application = manifest.application();
        let diagnostic = DiagnosticMetadata::try_from(manifest.with_name(name))?;
        let timestamp = diagnostic.collection_date;
        let cluster = ClusterMetadata::from(cluster);

        let temp = MetadataDocTemp {
            timestamp,
            cluster: &cluster,
            diagnostic: DiagnosticDocMetadata {
                metadata: &diagnostic,
                platform,
                application,
            },
            data_stream: DataStreamName::from("metrics-default-esdiag"),
        };
        let value = serde_json::to_value(&temp).expect("Failed to serialize metadata");
        let as_doc = MetadataValue(Arc::new(value));

        Ok(Self {
            as_doc,
            cluster,
            diagnostic,
            platform,
            application,
            timestamp,
        })
    }
}

#[derive(Clone)]
pub struct MetadataValue(pub Arc<Value>);

impl Serialize for MetadataValue {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // MetadataValue is always flattened, so we emit the pre-built JSON object directly.
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MetadataValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Value = Deserialize::deserialize(deserializer)?;
        Ok(MetadataValue(Arc::new(value)))
    }
}

impl MetadataValue {
    pub fn as_meta_doc(&self) -> Value {
        (*self.0).clone()
    }
}

impl Metadata for MetadataValue {
    fn as_meta_doc(&self) -> serde_json::Value {
        self.as_meta_doc()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) fn test_metadata(collection_date: &str) -> ElasticsearchMetadata {
        let manifest = DiagnosticManifest::new(
            collection_date.to_string(),
            None,
            None,
            None,
            None,
            Some(Application::Elasticsearch),
            None,
            None,
            Some("8.19.0".to_string()),
        );
        let cluster: Cluster = serde_json::from_value(json!({
            "name": "node-01",
            "cluster_name": "example",
            "cluster_uuid": "cluster-uuid",
            "tagline": "You Know, for Search",
            "version": {
                "number": "8.19.0",
                "build_type": "tar",
                "build_hash": "hash",
                "build_date": "2026-01-01T00:00:00Z",
                "build_snapshot": false,
                "lucene_version": "9.12.0",
                "minimum_wire_compatibility_version": "7.17.0",
                "minimum_index_compatibility_version": "7.0.0"
            }
        }))
        .expect("cluster");
        ElasticsearchMetadata::try_new(manifest, cluster.with_display_name(None)).expect("metadata")
    }

    #[test]
    fn sample_envelope_overrides_timestamp_only() {
        let metadata = test_metadata("2026-08-05T07:47:15.000Z");
        let collected = metadata.timestamp;
        let sample = 1_785_888_610_000;

        let envelope = metadata
            .for_data_stream_at("metrics-host.sar-esdiag", sample)
            .as_meta_doc();

        assert_eq!(envelope["@timestamp"], sample);
        assert_eq!(envelope["diagnostic"]["collection_date"], collected);
        assert_ne!(collected, sample);
        assert_eq!(envelope["data_stream"]["dataset"], "host.sar");
        assert_eq!(
            metadata.for_data_stream("metrics-host.sar-esdiag").as_meta_doc()["@timestamp"],
            collected
        );
    }
}
