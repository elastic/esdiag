use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use std::sync::RwLock;

use super::{DataSource, DiagPath, Manifest, Product, data_source::PathType};
use eyre::{Result, eyre};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct DiagnosticManifest {
    pub mode: Option<String>,
    pub product: Product,
    pub flags: Option<String>,
    pub diagnostic: Option<String>,
    pub r#type: Option<String>,
    pub runner: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "timestamp")]
    pub collection_date: String,
    pub collection_date_millis: Option<u64>,
    /// ECK diagnostic bunldes can contain multiple stack diagnostics
    pub included_diagnostics: Option<Vec<DiagPath>>,
    /// Name for human-readable IDs
    #[serde(skip_deserializing)]
    pub name: String,
    #[serde(skip_deserializing)]
    diagnostic_id: RwLock<Option<String>>,
}

impl DiagnosticManifest {
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
        let collection_date_millis = Some(0 as u64);
        let diagnostic_id = RwLock::new(None);
        let name = r#type.clone().unwrap_or("diagnostic".to_string());

        return Self {
            collection_date,
            collection_date_millis,
            diagnostic,
            diagnostic_id,
            flags,
            included_diagnostics,
            mode,
            name,
            product,
            r#type,
            runner,
            version,
        };
    }

    pub fn collection_date_in_millis(&self) -> u64 {
        if let Ok(date) = DateTime::parse_from_rfc3339(&self.collection_date) {
            date.timestamp_millis() as u64
        } else if let Ok(date) =
            DateTime::parse_from_str(&self.collection_date, "%Y-%m-%dT%H:%M:%S%.3f%z")
        {
            date.timestamp_millis() as u64
        } else {
            log::warn!("Failed to parse collection date: {}", &self.collection_date);
            chrono::Utc::now().timestamp_millis() as u64
        }
    }

    pub fn diagnostic_id(&self, uuid: &String) -> String {
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
}

impl DataSource for DiagnosticManifest {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("diagnostic_manifest.json"),
            _ => Err(eyre!("Unsupported source for manifest")),
        }
    }

    fn name() -> String {
        "diagnostic_manifest".to_string()
    }
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
