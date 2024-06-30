use super::Product;
use crate::processor::elasticsearch::{EsVersion, EsVersionDetails};
use semver;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub diagnostic_inputs: String,
    pub diag_version: Option<semver::Version>,
    #[serde(default)]
    pub product: Product,
    #[serde(rename = "Product Version")]
    pub product_version: ProductVersion,
    pub runner: Option<String>,
    pub collection_date: String,
}

impl Manifest {
    /// Infers manifest details from the `version.json` if there was no manifest
    pub fn from_es_version(version: EsVersion, date: SystemTime) -> Self {
        let product = match version.tagline.as_str() {
            "You Know, for Search" => Product::Elasticsearch,
            _ => unimplemented!("ERROR: Application not yet implemented"),
        };
        let product_version = ProductVersion::from(version.version);
        Self {
            collection_date: date
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                .to_string(),
            diag_version: None,
            diagnostic_inputs: "Unknown".to_string(),
            product,
            product_version,
            runner: Some("Unknown".to_string()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Version {
    Elasticsearch(EsVersion),
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductVersion {
    pub original_value: String,
    pub value: String,
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub suffix_tokens: Vec<String>,
    pub build: Option<String>,
    pub r#type: String,
    pub stable: bool,
}

impl ProductVersion {
    pub fn from(version: EsVersionDetails) -> Self {
        Self {
            original_value: version.number.to_string(),
            value: version.number.to_string(),
            major: version.number.major,
            minor: version.number.minor,
            patch: version.number.patch,
            suffix_tokens: vec![],
            build: Some(version.build_flavor),
            r#type: version.build_type,
            stable: true,
        }
    }
}
