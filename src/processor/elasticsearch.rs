pub mod cluster_settings;
pub mod index_settings;
pub mod index_stats;
pub mod lookup;
pub mod metadata;
pub mod nodes;
pub mod nodes_stats;
pub mod tasks;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EsDataSet {
    Alias,
    DataStreams,
    Nodes,
    Version,
    ClusterSettings,
    IlmExplain,
    IndexSettings,
    IndexStats,
    NodesStats,
    Tasks,
}

impl ToString for EsDataSet {
    fn to_string(&self) -> String {
        match self {
            EsDataSet::Alias => "alias".to_string(),
            EsDataSet::DataStreams => "data_stream".to_string(),
            EsDataSet::Nodes => "nodes".to_string(),
            EsDataSet::Version => "version".to_string(),
            EsDataSet::ClusterSettings => "cluster_settings_defaults".to_string(),
            EsDataSet::IlmExplain => "ilm_explain".to_string(),
            EsDataSet::IndexSettings => "settings".to_string(),
            EsDataSet::IndexStats => "indices_stats".to_string(),
            EsDataSet::NodesStats => "nodes_stats".to_string(),
            EsDataSet::Tasks => "tasks".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EsVersionDetails {
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

#[derive(Debug, Serialize, Deserialize)]
pub struct EsVersion {
    pub name: String,
    pub cluster_name: String,
    pub cluster_uuid: String,
    pub version: EsVersionDetails,
    pub tagline: String,
}
