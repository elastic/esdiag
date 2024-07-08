use super::Product;
use crate::processor::elasticsearch::{EsVersion, EsVersionDetails};
use crate::{input, Uri};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub diag_type: Option<String>,
    pub diagnostic_inputs: Option<String>,
    pub diag_version: Option<String>,
    #[serde(default)]
    pub product: Product,
    #[serde(rename = "Product Version")]
    pub product_version: Option<ProductVersion>,
    pub runner: Option<String>,
    pub collection_date: String,
    /// ECK diagnostic bunldes can contain multiple stack diagnostics
    pub included_diagnostics: Option<Vec<DiagPath>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagPath {
    pub diag_type: String,
    pub diag_path: String,
}

impl Manifest {
    /// Infers manifest details from the `version.json` if there was no manifest
    pub fn from_es_version(version: EsVersion, date: SystemTime) -> Self {
        let product = match version.tagline.as_str() {
            "You Know, for Search" => Product::Elasticsearch,
            _ => unimplemented!("ERROR: Application not implemented"),
        };
        let product_version = Some(ProductVersion::from(version.version));
        Self {
            diag_type: Some(String::from("es-unknown")),
            collection_date: date
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                .to_string(),
            diag_version: None,
            diagnostic_inputs: None,
            included_diagnostics: None,
            product,
            product_version,
            runner: Some("Unknown".to_string()),
        }
    }

    /// Loads a manifest from a URI
    pub fn from_uri(input_uri: &Uri) -> Result<Manifest, Box<dyn std::error::Error>> {
        let manifest: Manifest = match &input_uri {
            Uri::Directory(dir) => match input::file::read_string(&dir) {
                Ok(string) => serde_json::from_str::<Manifest>(&string)?.with_diag_type(),
                Err(e) => {
                    log::warn!(
                        "Failed to read manifest.json file, falling back to version.json: {e}"
                    );
                    let file_path = &dir.with_file_name("version.json");
                    let string = input::file::read_string(&file_path)?;
                    let date = std::fs::metadata(&file_path)?.created()?;
                    log::debug!("Got metadata for directory: {:?}", &date);
                    let version =
                        serde_json::from_str(&string).expect("Failed to parse version.json file");
                    Manifest::from_es_version(version, date)
                }
            },
            Uri::File(file) => match input::archive::read_string(&file, "manifest.json") {
                Ok(string) => serde_json::from_str::<Manifest>(&string)?.with_diag_type(),
                Err(e) => {
                    log::warn!(
                        "Failed to parse manifest.json file, falling back to version.json: {e}"
                    );
                    let string = input::archive::read_string(&file, "version.json")?;
                    let version =
                        serde_json::from_str(&string).expect("Failed to parse version.json file");
                    let date = std::fs::metadata(&file)?.created()?;
                    Manifest::from_es_version(version, date)
                }
            },
            _ => Err("Diagnostic manifest can only load from a local input")?,
        };
        Ok(manifest.with_product())
    }

    /// Updates product based on diag_type
    pub fn with_product(mut self) -> Self {
        log::debug!("Setting product from diag_type: {:?}", self.diag_type);
        self.product = match &self.diag_type {
            Some(diag_type) => match diag_type.as_str() {
                "api" | "local" | "remote" | "es-unknown" => Product::Elasticsearch,
                "kibana-api" => Product::Kibana,
                "logstash-api" => Product::Logstash,
                "eck-diagnostics" => Product::ECK,
                _ => Product::Unknown,
            },
            None => Product::Unknown,
        };
        self
    }

    /// Updates diag_type based on diagnostic_inputs
    pub fn with_diag_type(mut self) -> Self {
        if self.diag_type.is_some() {
            log::trace!("diag_type already set {:?}", self.diag_type);
            return self;
        }
        log::trace!(
            "Setting diag_type from diagnostic_inputs: {:?}",
            self.diagnostic_inputs
        );
        let re = Regex::new(r"diagType='([^']*)'").unwrap();
        match &self.diagnostic_inputs {
            Some(inputs) => {
                // Extract diag_type from diagnostic_inputs with regex
                self.diag_type = re
                    .captures(inputs)
                    .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()));
                self
            }
            None => self,
        }
    }
}

// Deserializing structs

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
