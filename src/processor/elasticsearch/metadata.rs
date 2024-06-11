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

impl Metadata {
    pub fn new(manifest: &Manifest, metadata: &HashMap<String, String>) -> Metadata {
        let version = metadata.get("version").expect("Failed to get version");
        let cluster: Cluster =
            serde_json::from_str(version).expect("Failed to deserialize version");

        let collection_date = DateTime::parse_from_rfc3339(&manifest.collection_date)
            .expect("Failed to parse collection_date")
            .timestamp_millis();

        let diagnostic = DiagnosticMetadata {
            collection_date,
            inputs: manifest.diagnostic_inputs.clone(),
            node: cluster.name.clone(),
            runner: manifest.runner.clone(),
            uuid: Uuid::new_v4().to_string(),
            version: manifest.diag_version.clone(),
        };

        let version = cluster.version.number.clone();
        Metadata {
            cluster,
            diagnostic,
            version: version,
            lookup: Lookups {
                alias: Lookup::<AliasData>::from(metadata["alias"].clone()),
                data_stream: Lookup::<DataStreamData>::from(metadata["data_stream"].clone()),
                index: Lookup::<IndexData>::new(),
                ilm: Lookup::<IlmData>::from(metadata["ilm_explain"].clone()),
                node: Lookup::<NodeData>::new(),
                shared_cache: Lookup::<SharedCacheStats>::from(
                    metadata["searchable_snapshots_cache_stats"].clone(),
                ),
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
