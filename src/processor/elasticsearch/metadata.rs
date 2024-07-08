use super::lookup::{
    alias::AliasDoc, data_stream::DataStreamDoc, ilm::IlmData, index::IndexData, node::NodeData,
    shared_cache::SharedCacheStats, Lookup,
};
use crate::input::manifest::Manifest;
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Serialize)]
pub struct Metadata {
    pub cluster: Cluster,
    pub diagnostic: DiagnosticDoc,
    pub version: semver::Version,
    pub lookup: Lookups,
    pub as_doc: MetadataDoc,
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

        let diagnostic = DiagnosticDoc {
            collection_date,
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
            as_doc,
            cluster,
            diagnostic,
            version,
            lookup: Lookups {
                alias: Lookup::<AliasDoc>::from(metadata["alias"].clone()),
                data_stream: match metadata.get("data_stream").clone() {
                    Some(data_stream) => Lookup::<DataStreamDoc>::from(data_stream),
                    None => Lookup::<DataStreamDoc>::new(),
                },
                index: Lookup::<IndexData>::new(),
                ilm: Lookup::<IlmData>::from(metadata["ilm_explain"].clone()),
                node: Lookup::<NodeData>::new(),
                shared_cache: match metadata.get("searchable_snapshots_cache_stats").clone() {
                    Some(cache) => Lookup::<SharedCacheStats>::from(cache),
                    None => Lookup::<SharedCacheStats>::new(),
                },
            },
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

#[derive(Clone, Serialize)]
pub struct Lookups {
    pub alias: Lookup<AliasDoc>,
    pub data_stream: Lookup<DataStreamDoc>,
    pub index: Lookup<IndexData>,
    pub node: Lookup<NodeData>,
    pub ilm: Lookup<IlmData>,
    pub shared_cache: Lookup<SharedCacheStats>,
}

// Serializing data structures

#[derive(Clone, Serialize)]
pub struct ClusterDoc {
    node_name: String,
    name: String,
    uuid: String,
    version: Version,
}

impl ClusterDoc {
    pub fn from(cluster: &Cluster) -> Self {
        Self {
            node_name: cluster.node_name.clone(),
            name: cluster.name.clone(),
            uuid: cluster.uuid.clone(),
            version: cluster.version.clone(),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct MetadataDoc {
    #[serde(rename = "@timestamp")]
    pub timestamp: i64,
    pub cluster: ClusterDoc,
    pub diagnostic: DiagnosticDoc,
}

#[derive(Clone, Serialize)]
pub struct DiagnosticDoc {
    pub collection_date: i64,
    pub node: String,
    pub runner: String,
    pub uuid: String,
    pub version: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct DataStream {
    dataset: String,
    namespace: String,
    r#type: String,
}

impl From<&str> for DataStream {
    fn from(name: &str) -> Self {
        let terms: Vec<&str> = name.split('-').collect();
        DataStream {
            r#type: terms[0].to_string(),
            dataset: terms[1].to_string(),
            namespace: terms[2].to_string(),
        }
    }
}

// Deserializing data structures

#[derive(Clone, Deserialize, Serialize)]
pub struct Cluster {
    #[serde(rename = "name")]
    pub node_name: String,
    #[serde(rename = "cluster_name")]
    pub name: String,
    #[serde(rename = "cluster_uuid")]
    pub uuid: String,
    pub version: Version,
    pub tagline: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Version {
    pub number: semver::Version,
    pub build_flavor: String,
    pub build_type: String,
    pub build_hash: String,
    pub build_date: String,
    pub build_snapshot: bool,
    pub lucene_version: String,
    pub minimum_wire_compatibility_version: String,
    pub minimum_index_compatibility_version: String,
}
