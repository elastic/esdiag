/// Trait for receiving data from a source
pub mod data_source;
/// Modern diagnostic bundle manifest file
pub mod diagnostic_manifest;
/// Elastic Cloud Kubernetes diagnostic bundle
pub mod eck;
/// Elasticsearch diagnostic bundle
pub mod elasticsearch;
/// Kibana diagnostic bundle
pub mod kibana;
/// Logstash diagnostic bundle
pub mod logstash;
/// Legacy diagnostic bundle manifest file
pub mod manifest;

use std::str::FromStr;

pub use diagnostic_manifest::DiagnosticManifest;
pub use eck::ElasticCloudKubernetes;
pub use elasticsearch::Elasticsearch;
pub use kibana::Kibana;
pub use logstash::Logstash;
pub use manifest::Manifest;

use elasticsearch::ElasticsearchDataSet;
use serde::{Deserialize, Deserializer, Serialize};

pub trait DataFamilies {
    fn get_data_sets(&self) -> Vec<DataSet>;
    fn get_lookup_sets(&self) -> Vec<DataSet>;
    fn get_metadata_sets(&self) -> Vec<DataSet>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataSet {
    Elasticsearch(ElasticsearchDataSet),
    //Kibana(KibanaDataSet),
    //Logstash(LogstashDataSet),
}

impl ToString for DataSet {
    fn to_string(&self) -> String {
        match self {
            DataSet::Elasticsearch(data_set) => data_set.to_string(),
            //DataSet::Kibana(data_set) => data_set.to_string(),
            //DataSet::Logstash(data_set) => data_set.to_string(),
        }
    }
}

// Product enum to hold the Elasticsearch, Kibana, or Logstash product

#[derive(Debug, PartialEq, Hash, Clone, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Product {
    Agent,
    ECE,
    ECK,
    Elasticsearch,
    Kibana,
    Logstash,
    Unknown,
}

impl std::fmt::Display for Product {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Agent => write!(fmt, "Agent"),
            Self::ECE => write!(fmt, "ECE"),
            Self::ECK => write!(fmt, "ECK"),
            Self::Elasticsearch => write!(fmt, "Elasticsearch"),
            Self::Kibana => write!(fmt, "Kibana"),
            Self::Logstash => write!(fmt, "Logstash"),
            Self::Unknown => write!(fmt, "Unknown"),
        }
    }
}

impl std::str::FromStr for Product {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "agent" => Ok(Self::Agent),
            "ece" => Ok(Self::ECE),
            "eck" => Ok(Self::ECK),
            "es" | "elasticsearch" => Ok(Self::Elasticsearch),
            "kb" | "kibana" => Ok(Self::Kibana),
            "ls" | "logstash" => Ok(Self::Logstash),
            _ => Err("Unknown product".to_string()),
        }
    }
}

// Custom case-insensitve deserialization for the Product enum
impl<'de> Deserialize<'de> for Product {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize as a string
        let s: String = Deserialize::deserialize(deserializer)?;

        // Normalize the string to lowercase to match
        Product::from_str(&s.to_lowercase())
            .map_err(|e| serde::de::Error::custom(format!("Unknown product: {}", e)))
    }
}

impl Default for Product {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagPath {
    pub diag_type: String,
    pub diag_path: String,
}
