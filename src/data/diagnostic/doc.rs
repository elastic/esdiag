use super::DiagnosticManifest;
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use color_eyre::eyre::Result;
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Serialize)]
pub struct DiagnosticDoc {
    pub collection_date: i64,
    pub runner: String,
    pub id: String,
    pub uuid: String,
    pub version: Option<String>,
}

impl DiagnosticDoc {
    pub fn new(
        collection_date: i64,
        id: String,
        runner: String,
        uuid: String,
        version: Option<String>,
    ) -> Self {
        DiagnosticDoc {
            collection_date,
            runner,
            id,
            uuid,
            version,
        }
    }
}

impl TryFrom<DiagnosticManifest> for DiagnosticDoc {
    type Error = color_eyre::eyre::Report;

    fn try_from(manifest: DiagnosticManifest) -> Result<Self> {
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

        let collection_date_string = Utc
            .timestamp_millis_opt(collection_date)
            .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Secs, true))
            .unwrap();

        let uuid = Uuid::new_v4().to_string();
        // Human readable ID
        let id = format!(
            "{}@{}#{}",
            manifest.name.expect("Diagnostic name not found"),
            collection_date_string,
            uuid.chars().take(4).collect::<String>()
        );

        let runner = match &manifest.runner {
            Some(runner) => runner.clone(),
            None => "Unknown".to_string(),
        };

        Ok(DiagnosticDoc::new(
            collection_date,
            id,
            runner,
            uuid,
            manifest.version,
        ))
    }
}
