use super::lookup::{index::IndexData, node::NodeData, Lookup};
use crate::data::{
    diagnostic::Manifest,
    elasticsearch::{Alias, Cluster, DataStream, IlmStats, SharedCacheStats},
};
use chrono::DateTime;
use serde::Serialize;
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

        let version = cluster.version.number.clone();

        let as_doc = MetadataDoc {
            timestamp: diagnostic.collection_date,
            cluster: cluster.clone(),
            diagnostic: diagnostic.clone(),
        };

        Metadata {
            as_doc,
            cluster,
            diagnostic,
            version,
            lookup: Lookups {
                alias: Lookup::<Alias>::from(metadata["alias"].clone()),
                data_stream: match metadata.get("data_stream").clone() {
                    Some(data_stream) => Lookup::<DataStream>::from(data_stream),
                    None => Lookup::<DataStream>::new(),
                },
                index_settings: Lookup::<IndexData>::new(),
                ilm_explain: Lookup::<IlmStats>::from(metadata["ilm_explain"].clone()),
                node: Lookup::<NodeData>::new(),
                shared_cache: match metadata.get("searchable_snapshots_cache_stats").clone() {
                    Some(cache) => Lookup::<SharedCacheStats>::from(cache),
                    None => Lookup::<SharedCacheStats>::new(),
                },
            },
        }
    }
}

#[derive(Clone, Serialize)]
pub struct Lookups {
    pub alias: Lookup<Alias>,
    pub data_stream: Lookup<DataStream>,
    pub index_settings: Lookup<IndexData>,
    pub node: Lookup<NodeData>,
    pub ilm_explain: Lookup<IlmStats>,
    pub shared_cache: Lookup<SharedCacheStats>,
}

// Serializing data structures

#[derive(Clone, Serialize)]
pub struct MetadataDoc {
    #[serde(rename = "@timestamp")]
    pub timestamp: i64,
    pub cluster: Cluster,
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
pub struct DataStreamName {
    dataset: String,
    namespace: String,
    r#type: String,
}

impl From<&str> for DataStreamName {
    fn from(name: &str) -> Self {
        let terms: Vec<&str> = name.split('-').collect();
        DataStreamName {
            r#type: terms[0].to_string(),
            dataset: terms[1].to_string(),
            namespace: terms[2].to_string(),
        }
    }
}
