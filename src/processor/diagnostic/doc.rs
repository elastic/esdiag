use super::DiagnosticManifest;
use eyre::Result;
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Serialize)]
pub struct DiagnosticMetadata {
    pub collection_date: u64,
    pub runner: String,
    pub id: String,
    pub uuid: String,
    pub version: Option<String>,
}

impl DiagnosticMetadata {
    pub fn new(
        collection_date: u64,
        id: String,
        runner: String,
        uuid: String,
        version: Option<String>,
    ) -> Self {
        DiagnosticMetadata {
            collection_date,
            runner,
            id,
            uuid,
            version,
        }
    }
}

impl TryFrom<DiagnosticManifest> for DiagnosticMetadata {
    type Error = eyre::Report;

    fn try_from(manifest: DiagnosticManifest) -> Result<Self> {
        let runner = match &manifest.runner {
            Some(runner) => runner.clone(),
            None => "Unknown".to_string(),
        };

        let uuid = Uuid::new_v4().to_string();

        Ok(DiagnosticMetadata::new(
            manifest.collection_date_in_millis(),
            manifest.diagnostic_id(&uuid).clone(),
            runner,
            uuid,
            manifest.version,
        ))
    }
}
