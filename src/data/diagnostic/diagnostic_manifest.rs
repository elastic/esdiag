use super::{data_source::DataSource, DiagPath, Manifest, Product};
use crate::data::Uri;
use color_eyre::eyre::{eyre, Result};
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
    /// ECK diagnostic bunldes can contain multiple stack diagnostics
    pub included_diagnostics: Option<Vec<DiagPath>>,
}

impl DataSource for DiagnosticManifest {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) => Ok("diagnostic_manifest.json"),
            Uri::File(_) => Ok("diagnostic_manifest.json"),
            _ => Err(eyre!("Unsupported source for manifest")),
        }
    }

    fn name() -> &'static str {
        "diagnostic_manifest"
    }
}

impl From<Manifest> for DiagnosticManifest {
    fn from(manifest: Manifest) -> Self {
        Self {
            mode: manifest.diag_type,
            product: manifest.product,
            flags: manifest.diagnostic_inputs,
            diagnostic: manifest.diag_version,
            r#type: Some("unknown".to_string()),
            runner: manifest.runner,
            version: manifest
                .product_version
                .map(|v| v.original_value.map(|v| v.to_string()).unwrap_or_default()),
            collection_date: manifest.collection_date,
            included_diagnostics: manifest.included_diagnostics,
        }
    }
}
