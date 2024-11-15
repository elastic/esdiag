use super::{DataSource, DiagPath, Manifest, Product};
use crate::data::{diagnostic::DataSet, Uri};
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
    /// Name for human-readable IDs
    pub name: Option<String>,
}

impl DiagnosticManifest {
    pub fn with_name(self, name: String) -> Self {
        Self {
            name: Some(name),
            ..self
        }
    }
}

impl DataSource for DiagnosticManifest {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) => Ok("diagnostic_manifest.json"),
            Uri::File(_) => Ok("diagnostic_manifest.json"),
            _ => Err(eyre!("Unsupported source for manifest")),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::DiagnosticManifest)
    }
}

impl From<Manifest> for DiagnosticManifest {
    fn from(manifest: Manifest) -> Self {
        let product = match manifest.diag_type.as_deref() {
            Some("eck-diagnostics") => Product::ECK,
            _ => Product::Elasticsearch,
        };
        Self {
            mode: Some("compatible".to_string()),
            product,
            flags: manifest.diagnostic_inputs,
            diagnostic: manifest.diag_version,
            r#type: manifest.diag_type,
            runner: manifest.runner,
            version: manifest
                .product_version
                .map(|v| v.original_value.map(|v| v.to_string()).unwrap_or_default()),
            collection_date: manifest.collection_date,
            included_diagnostics: manifest.included_diagnostics,
            name: None,
        }
    }
}
