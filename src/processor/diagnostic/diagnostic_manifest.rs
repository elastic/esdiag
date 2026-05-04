// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::Identifiers;
use super::{DiagPath, Manifest};
use crate::data::Product;
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RequestedApi {
    /// Final HTTP response status observed for this API request
    pub status: u16,
    /// Number of retry attempts performed before the final response
    pub retries: u32,
    /// Time spent waiting for the final response body
    pub response_time_ms: u64,
    /// Size in bytes of the final response body
    pub response_size_bytes: u64,
}

#[derive(Deserialize, Serialize)]
pub struct DiagnosticManifest {
    /// Diagnostic bundle variation
    pub mode: Option<String>,
    /// Elastic Stack component name
    pub product: Product,
    /// Command-line flags used when running the diagnostic collector
    pub flags: Option<String>,
    /// Diagnostic collector version
    pub diagnostic: Option<String>,
    /// Diagnostic type (relates to product, not mode)
    pub r#type: Option<String>,
    /// Where the diagnostic was run from
    pub runner: Option<String>,
    /// Elastic Stack version
    pub version: Option<String>,
    /// Datetime when the diagnostic was collected
    #[serde(rename = "timestamp")]
    pub collection_date: String,
    /// Collection time in milliseconds since the Unix epoch
    pub collection_date_millis: Option<u64>,
    /// Platform diagnostic bundles can contain multiple diagnostics from different components
    pub included_diagnostics: Option<Vec<DiagPath>>,
    /// Name for human-readable IDs
    #[serde(skip_deserializing)]
    pub name: String,
    #[serde(skip_deserializing)]
    diagnostic_id: RwLock<Option<String>>,
    /// Additional identifiers not included in the diagnostic itself
    pub identifiers: Option<Identifiers>,
    /// APIs requested during this run keyed by API name
    pub requested_apis: Option<HashMap<String, RequestedApi>>,
}

impl Clone for DiagnosticManifest {
    fn clone(&self) -> Self {
        let diagnostic_id = if let Ok(id) = self.diagnostic_id.read() {
            RwLock::new(id.clone())
        } else {
            RwLock::new(None)
        };
        Self {
            mode: self.mode.clone(),
            product: self.product.clone(),
            flags: self.flags.clone(),
            diagnostic: self.diagnostic.clone(),
            r#type: self.r#type.clone(),
            runner: self.runner.clone(),
            version: self.version.clone(),
            collection_date: self.collection_date.clone(),
            collection_date_millis: self.collection_date_millis,
            included_diagnostics: self.included_diagnostics.clone(),
            name: self.name.clone(),
            diagnostic_id,
            identifiers: self.identifiers.clone(),
            requested_apis: self.requested_apis.clone(),
        }
    }
}

impl DiagnosticManifest {
    fn parse_collection_date_millis(collection_date: &str) -> Option<u64> {
        if let Ok(date) = DateTime::parse_from_rfc3339(collection_date) {
            Some(date.timestamp_millis() as u64)
        } else if let Ok(date) = DateTime::parse_from_str(collection_date, "%Y-%m-%dT%H:%M:%S%.3f%z") {
            Some(date.timestamp_millis() as u64)
        } else {
            None
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        collection_date: String,
        diagnostic: Option<String>,
        flags: Option<String>,
        included_diagnostics: Option<Vec<DiagPath>>,
        mode: Option<String>,
        product: Product,
        r#type: Option<String>,
        runner: Option<String>,
        version: Option<String>,
    ) -> Self {
        let collection_date_millis = Some(
            Self::parse_collection_date_millis(&collection_date).unwrap_or_else(|| {
                tracing::warn!("Failed to parse collection date: {}", &collection_date);
                chrono::Utc::now().timestamp_millis() as u64
            }),
        );
        let diagnostic_id = RwLock::new(None);
        let name = r#type.clone().unwrap_or("diagnostic".to_string());

        Self {
            collection_date,
            collection_date_millis,
            diagnostic,
            diagnostic_id,
            flags,
            included_diagnostics,
            identifiers: None,
            requested_apis: None,
            mode,
            name,
            product,
            r#type,
            runner,
            version,
        }
    }

    pub fn collection_date_in_millis(&self) -> u64 {
        self.collection_date_millis.unwrap_or_else(|| {
            Self::parse_collection_date_millis(&self.collection_date).unwrap_or_else(|| {
                tracing::warn!("Failed to parse collection date: {}", &self.collection_date);
                chrono::Utc::now().timestamp_millis() as u64
            })
        })
    }

    pub fn diagnostic_id(&self, uuid: &str) -> String {
        let mut id = self
            .diagnostic_id
            .write()
            .expect("Failed to obtain write lock for diagnostic id");

        match id.as_ref() {
            Some(id) => id.clone(),
            None => {
                let collection_date_string = Utc
                    .timestamp_millis_opt(self.collection_date_in_millis() as i64)
                    .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Secs, true))
                    .unwrap();

                // Human readable ID
                *id = Some(format!(
                    "{}@{}~{}",
                    self.name,
                    &collection_date_string[..10], // Trim to only date
                    &uuid[..4]
                ));
                id.clone().unwrap()
            }
        }
    }

    pub fn with_name(self, name: String) -> Self {
        Self { name, ..self }
    }

    pub fn with_identifiers(self, identifiers: Identifiers) -> Self {
        Self {
            identifiers: Some(identifiers),
            ..self
        }
    }

    pub fn with_requested_apis(self, requested_apis: HashMap<String, RequestedApi>) -> Self {
        Self {
            requested_apis: Some(requested_apis),
            ..self
        }
    }
}

impl DiagnosticManifest {
    pub const FILENAME: &'static str = "diagnostic_manifest.json";
}

impl From<Manifest> for DiagnosticManifest {
    fn from(manifest: Manifest) -> Self {
        let product = match manifest.diag_type.as_deref() {
            Some("eck-diagnostics") => Product::ECK,
            Some("k8s-platform-diagnostics") => Product::KubernetesPlatform,
            _ => Product::Elasticsearch,
        };
        DiagnosticManifest::new(
            manifest.collection_date,
            manifest.diag_version,
            manifest.diagnostic_inputs,
            manifest.included_diagnostics,
            Some("compatible".to_string()),
            product,
            manifest.diag_type,
            manifest.runner,
            manifest
                .product_version
                .map(|v| v.original_value.map(|v| v.to_string()).unwrap_or_default()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::DiagnosticManifest;
    use crate::data::Product;

    #[test]
    fn new_sets_collection_date_millis_from_timestamp() {
        let manifest = DiagnosticManifest::new(
            "2026-04-25T20:18:43.610Z".to_string(),
            Some("esdiag-0.15.0-SNAPSHOT".to_string()),
            None,
            None,
            Some("support".to_string()),
            Product::Elasticsearch,
            Some("elasticsearch_diagnostic".to_string()),
            Some("esdiag".to_string()),
            Some("8.19.3".to_string()),
        );

        assert_eq!(manifest.collection_date_millis, Some(1_777_148_323_610));
    }

    #[test]
    fn collection_date_in_millis_uses_stored_value_first() {
        let mut manifest = DiagnosticManifest::new(
            "2026-04-25T20:18:43.610Z".to_string(),
            Some("esdiag-0.15.0-SNAPSHOT".to_string()),
            None,
            None,
            Some("support".to_string()),
            Product::Elasticsearch,
            Some("elasticsearch_diagnostic".to_string()),
            Some("esdiag".to_string()),
            Some("8.19.3".to_string()),
        );
        manifest.collection_date = "not-a-date".to_string();

        assert_eq!(manifest.collection_date_in_millis(), 1_777_148_323_610);
    }
}
