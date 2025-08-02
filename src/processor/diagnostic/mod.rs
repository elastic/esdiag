/// Trait for receiving data from a source
pub mod data_source;
/// Data stream naming
pub mod data_stream_name;
/// Modern diagnostic bundle manifest file
pub mod diagnostic_manifest;
/// Diagnostic metada doc
pub mod doc;
/// Diagnostic lookup tables
pub mod lookup;
/// Legacy diagnostic bundle manifest file
pub mod manifest;
/// Diagnostic job report
pub mod report;

pub use data_source::DataSource;
pub use data_stream_name::DataStreamName;
pub use diagnostic_manifest::DiagnosticManifest;
pub use doc::DiagnosticMetadata;
pub use lookup::Lookup;
pub use manifest::Manifest;
pub use report::{DiagnosticReport, DiagnosticReportBuilder};
use serde::{Deserialize, Deserializer, Serialize};
use std::str::FromStr;

#[derive(Debug, PartialEq, Hash, Clone, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Product {
    Agent,
    ECE,
    ECK,
    Elasticsearch,
    Kibana,
    KubernetesPlatform,
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
            Self::KubernetesPlatform => write!(fmt, "KubernetesPlatform"),
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
            "mki" => Ok(Self::KubernetesPlatform),
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
        Self::Elasticsearch
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagPath {
    pub diag_type: String,
    pub diag_path: String,
}
