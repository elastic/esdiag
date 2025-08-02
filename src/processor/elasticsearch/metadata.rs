use super::super::diagnostic::{DataStreamName, DiagnosticManifest, DiagnosticMetadata};
use super::{Metadata, version::Cluster};
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Serialize)]
pub struct ElasticsearchMetadata {
    pub cluster: Cluster,
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

#[derive(Clone, Serialize)]
pub struct MetadataDoc {
    #[serde(rename = "@timestamp")]
    pub timestamp: u64,
    pub cluster: Cluster,
    pub diagnostic: DiagnosticMetadata,
    pub data_stream: DataStreamName,
}

impl Metadata for MetadataDoc {
    fn as_meta_doc(&self) -> Value {
        serde_json::to_value(&self).expect("Failed to serialize metadata")
    }
}

impl ElasticsearchMetadata {
    pub fn try_new(manifest: DiagnosticManifest, cluster: Cluster) -> Result<Self> {
        let name = cluster.display_name.replace(" ", "_");
        let diagnostic = DiagnosticMetadata::try_from(manifest.with_name(name))?;
        let timestamp = diagnostic.collection_date;

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

#[derive(Serialize, Deserialize)]
pub struct ElasticsearchVersion {
    pub name: String,
    pub cluster_name: String,
    pub cluster_uuid: String,
    pub version: ElasticsearchVersionDetails,
    pub tagline: String,
}

#[derive(Serialize, Deserialize)]
pub struct ElasticsearchVersionDetails {
    pub number: semver::Version,
    pub build_flavor: String,
    pub build_type: String,
    pub build_hash: String,
    pub build_date: String,
    pub build_snapshot: bool,
    pub lucene_version: String,
    pub minimum_wire_compatibility_version: semver::Version,
    pub minimum_index_compatibility_version: semver::Version,
}
