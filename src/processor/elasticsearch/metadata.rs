use super::lookup::{
    alias::AliasData, data_stream::DataStreamData, ilm::IlmData, index::IndexData, node::NodeData,
    shared_cache::SharedCacheStats, Lookup,
};
use crate::input::manifest::Manifest;
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticMetadata {
    pub collection_date: i64,
    pub inputs: String,
    pub node: String,
    pub runner: String,
    pub uuid: String,
    pub version: Option<semver::Version>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Lookups {
    pub alias: Lookup<AliasData>,
    pub data_stream: Lookup<DataStreamData>,
    pub index: Lookup<IndexData>,
    pub node: Lookup<NodeData>,
    pub ilm: Lookup<IlmData>,
    pub shared_cache: Lookup<SharedCacheStats>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Metadata {
    pub cluster: Cluster,
    pub diagnostic: DiagnosticMetadata,
    pub version: semver::Version,
    pub lookup: Lookups,
    pub as_doc: MetadataDoc,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Cluster {
    #[serde(rename = "name")]
    node_name: String,
    #[serde(rename = "cluster_name")]
    name: String,
    #[serde(rename = "cluster_uuid")]
    uuid: String,
    version: Version,
    tagline: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Version {
    number: semver::Version,
    build_flavor: String,
    build_type: String,
    build_hash: String,
    build_date: String,
    build_snapshot: bool,
    lucene_version: String,
    minimum_wire_compatibility_version: String,
    minimum_index_compatibility_version: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClusterDoc {
    node_name: String,
    name: String,
    uuid: String,
    version: String,
}

impl ClusterDoc {
    pub fn from(cluster: &Cluster) -> Self {
        Self {
            node_name: cluster.node_name.clone(),
            name: cluster.name.clone(),
            uuid: cluster.uuid.clone(),
            version: cluster.version.number.to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MetadataDoc {
    #[serde(rename = "@timestamp")]
    timestamp: i64,
    cluster: ClusterDoc,
    diagnostic: DiagnosticMetadata,
}

impl Metadata {
    pub fn new(manifest: &Manifest, metadata: HashMap<String, String>) -> Metadata {
        let version = metadata.get("version").expect("Failed to get version");
        let cluster: Cluster =
            serde_json::from_str(version).expect("Failed to deserialize version");

        let collection_date = {
            if let Ok(date) = DateTime::parse_from_rfc3339(&manifest.collection_date) {
                date.timestamp_millis()
            } else if let Ok(date) =
                DateTime::parse_from_str(&manifest.collection_date, "%Y-%m-%dT%H:%M:%S%.3f%z")
            {
                date.timestamp_millis()
            } else {
                log::warn!(
                    "Failed to parse collection date: {}",
                    manifest.collection_date
                );
                chrono::Utc::now().timestamp_millis()
            }
        };

        let runner = match &manifest.runner {
            Some(runner) => runner.clone(),
            None => "Unknown".to_string(),
        };

        let diagnostic = DiagnosticMetadata {
            collection_date,
            inputs: manifest.diagnostic_inputs.clone(),
            node: cluster.name.clone(),
            runner,
            uuid: Uuid::new_v4().to_string(),
            version: manifest.diag_version.clone(),
        };

        let as_doc = MetadataDoc {
            timestamp: diagnostic.collection_date,
            cluster: ClusterDoc::from(&cluster),
            diagnostic: diagnostic.clone(),
        };

        let version = cluster.version.number.clone();
        Metadata {
            cluster,
            diagnostic,
            version,
            lookup: Lookups {
                alias: Lookup::<AliasData>::from(metadata["alias"].clone()),
                data_stream: match metadata.get("data_stream").clone() {
                    Some(data_stream) => Lookup::<DataStreamData>::from(data_stream),
                    None => Lookup::<DataStreamData>::new(),
                },
                index: Lookup::<IndexData>::new(),
                ilm: Lookup::<IlmData>::from(metadata["ilm_explain"].clone()),
                node: Lookup::<NodeData>::new(),
                shared_cache: match metadata.get("searchable_snapshots_cache_stats").clone() {
                    Some(cache) => Lookup::<SharedCacheStats>::from(cache),
                    None => Lookup::<SharedCacheStats>::new(),
                },
            },
            as_doc,
        }
    }

    pub fn to_hashmap(&self) -> HashMap<String, Value> {
        let hashmap = HashMap::from([
            ("cluster".to_string(), json!(self.cluster)),
            ("diagnostic".to_string(), json!(self.diagnostic)),
            (
                "version".to_string(),
                json!({"version": self.version.clone()}),
            ),
            ("alias_lookup".to_string(), self.lookup.alias.to_value()),
            (
                "data_stream_lookup".to_string(),
                self.lookup.data_stream.to_value(),
            ),
            ("index_lookup".to_string(), self.lookup.index.to_value()),
            ("ilm_lookup".to_string(), self.lookup.ilm.to_value()),
            ("node_lookup".to_string(), self.lookup.node.to_value()),
            (
                "shared_cache_lookup".to_string(),
                self.lookup.shared_cache.to_value(),
            ),
        ]);
        hashmap
    }
}
