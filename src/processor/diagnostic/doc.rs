// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::DiagnosticManifest;
use eyre::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
pub struct DiagnosticMetadata {
    /// Date the diagnostic was originally collected
    pub collection_date: u64,
    /// Collection utility or method
    pub runner: String,
    /// User-friendly diagnostic identifier
    pub id: String,
    /// Unique identifier
    pub uuid: String,
}

impl DiagnosticMetadata {
    pub fn new(collection_date: u64, id: String, runner: String, uuid: String) -> Self {
        DiagnosticMetadata {
            collection_date,
            runner,
            id,
            uuid,
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
            manifest.diagnostic_id(&uuid),
            runner,
            uuid,
        ))
    }
}
