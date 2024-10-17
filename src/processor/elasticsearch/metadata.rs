use super::Metadata;
use crate::data::{
    diagnostic::DiagnosticManifest,
    elasticsearch::{Cluster, DataStreamName},
};
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use color_eyre::eyre::Result;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Serialize)]
pub struct ElasticsearchMetadata {
    pub cluster: Cluster,
    pub diagnostic: DiagnosticDoc,
    pub timestamp: i64,
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
    pub timestamp: i64,
    pub cluster: Cluster,
    pub diagnostic: DiagnosticDoc,
    pub data_stream: DataStreamName,
}

impl Metadata for MetadataDoc {
    fn as_meta_doc(&self) -> Value {
        serde_json::to_value(&self).expect("Failed to serialize metadata")
    }
}

#[derive(Clone, Serialize)]
pub struct DiagnosticDoc {
    pub collection_date: i64,
    pub node: String,
    pub runner: String,
    pub id: String,
    pub uuid: String,
    pub version: Option<String>,
}

impl ElasticsearchMetadata {
    pub fn try_new(manifest: DiagnosticManifest, cluster: Cluster) -> Result<Self> {
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

        let collection_date_string = Utc
            .timestamp_millis_opt(collection_date)
            .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Secs, true))
            .unwrap();

        // Create a human readable diagnostic ID
        let name = match &cluster.display_name {
            Some(name) if &runner == "ess" => {
                let mut parts = name.split_whitespace().collect::<Vec<&str>>();
                parts.pop();
                parts.join("_")
            }
            Some(name) => name.replace(" ", "_"),
            None => cluster.name.replace(" ", "_"),
        };
        let uuid = Uuid::new_v4().to_string();
        let hash = uuid.chars().take(4).collect::<String>();
        let id = format!("{}@{}#{}", name, collection_date_string, hash);

        let diagnostic = DiagnosticDoc {
            collection_date: collection_date.clone(),
            id,
            node: cluster.name.clone(),
            runner,
            uuid,
            version: manifest.diagnostic.clone(),
        };

        let as_doc = MetadataDoc {
            timestamp: collection_date.clone(),
            cluster: cluster.clone(),
            diagnostic: diagnostic.clone(),
            data_stream: DataStreamName::from("metrics-default-esdiag"),
        };

        Ok(Self {
            as_doc,
            cluster,
            diagnostic,
            timestamp: collection_date.clone(),
        })
    }
}
