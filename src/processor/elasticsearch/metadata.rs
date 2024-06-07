use super::lookup::{index::IndexLookup, node::NodeLookup, Lookup};
use super::EsDataSet::*;
use crate::input::manifest::Manifest;
use crate::input::DataSet::Elasticsearch;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticMetadata {
    pub collection_date: String,
    pub inputs: String,
    pub node: String,
    pub runner: String,
    pub uuid: String,
    pub version: semver::Version,
}

#[derive(Clone, Debug, Serialize)]
pub struct Lookups {
    pub alias: Lookup,
    pub data_stream: Lookup,
    pub index: Lookup,
    pub node: Lookup,
}

#[derive(Clone, Debug, Serialize)]
pub struct Metadata {
    pub cluster: Value,
    pub diagnostic: DiagnosticMetadata,
    pub version: String,
    pub lookup: Lookups,
}

impl Metadata {
    pub fn new(manifest: &Manifest, metadata: &HashMap<String, Value>) -> Metadata {
        let version = &metadata["version"];

        let cluster = json!({
            "name": version["cluster_name"],
            "uuid": version["cluster_uuid"],
            "version": version["version"]["number"],
            "build": {
                "flavor": version["version"]["build_flavor"],
                "type": version["version"]["build_type"],
                "hash": version["version"]["build_hash"],
                "date": version["version"]["build_date"],
                "snapshot": version["version"]["build_snapshot"],
                "lucene_version": version["version"]["lucene_version"],
                "minimum_wire_compatibility_version": version["version"]["minimum_wire_compatibility_version"],
                "minimum_index_compatibility_version": version["version"]["minimum_index_compatibility_version"],
            }
        });

        let diagnostic = DiagnosticMetadata {
            collection_date: manifest.collection_date.clone(),
            inputs: manifest.diagnostic_inputs.clone(),
            node: version["name"]
                .as_str()
                .expect("Failed to get version.name")
                .to_string(),
            runner: manifest.runner.clone(),
            uuid: Uuid::new_v4().to_string(),
            version: manifest
                .diag_version
                .clone()
                .unwrap_or(semver::Version::new(0, 0, 0)),
        };

        Metadata {
            cluster,
            diagnostic,
            version: version["version"]["number"]
                .as_str()
                .expect("Failed to get version.number")
                .to_string(),
            lookup: Lookups {
                alias: Lookup::from_value(Elasticsearch(Alias), metadata["alias"].clone()),
                data_stream: Lookup::from_value(
                    Elasticsearch(DataStreams),
                    metadata["data_stream"].clone(),
                ),
                index: Lookup::IndexLookup(IndexLookup::new()),
                node: Lookup::NodeLookup(NodeLookup::new()),
            },
        }
    }

    pub fn to_hashmap(&self) -> HashMap<String, Value> {
        let hashmap = HashMap::from([
            ("cluster".to_string(), self.cluster.clone()),
            ("diagnostic".to_string(), json!(self.diagnostic)),
            //("version".to_string(), Value::from(self.version.clone())),
            ("alias_lookup".to_string(), self.lookup.alias.to_value()),
            (
                "data_stream_lookup".to_string(),
                self.lookup.data_stream.to_value(),
            ),
            ("index_lookup".to_string(), self.lookup.index.to_value()),
            ("node_lookup".to_string(), self.lookup.node.to_value()),
        ]);
        hashmap
    }
}
