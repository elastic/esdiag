use super::{data_source::DataSource, elasticsearch::ElasticsearchVersion, DiagPath, Product};
use crate::{data::elasticsearch, data::Uri};
use color_eyre::eyre::{eyre, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

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

impl Manifest {
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
            None => match &self.runner {
                Some(runner) => match runner.as_str() {
                    "ess" => Product::Elasticsearch,
                    "eck" => Product::ECK,
                    _ => Product::Unknown,
                },
                None => Product::Unknown,
            },
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
    Elasticsearch(ElasticsearchVersion),
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductVersion {
    pub original_value: Option<String>,
    pub value: Option<String>,
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub suffix_tokens: Option<Vec<String>>,
    pub pre_release: Option<Vec<String>>,
    //pub build: Option<String>,
    pub r#type: Option<String>,
    pub stable: bool,
}

impl From<elasticsearch::Version> for ProductVersion {
    fn from(version: elasticsearch::Version) -> Self {
        Self {
            original_value: Some(version.number.to_string().clone()),
            value: Some(version.number.to_string().clone()),
            major: version.number.major,
            minor: version.number.minor,
            patch: version.number.patch,
            suffix_tokens: Some(vec![]),
            pre_release: None,
            //build: Some(version.build_flavor),
            r#type: Some(version.build_type),
            stable: true,
        }
    }
}

impl TryFrom<elasticsearch::Cluster> for Manifest {
    type Error = color_eyre::eyre::Error;

    /// Create a manifest from a cluster's metadata (`version.json`) file
    fn try_from(cluster: elasticsearch::Cluster) -> Result<Self, Self::Error> {
        Ok(Self {
            diag_type: None,
            diagnostic_inputs: None,
            diag_version: None,
            product: Product::Elasticsearch,
            product_version: Some(ProductVersion::from(cluster.version)),
            runner: None,
            collection_date: chrono::Utc::now().to_rfc3339(),
            included_diagnostics: None,
        })
    }
}

impl DataSource for Manifest {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) => Ok("manifest.json"),
            Uri::File(_) => Ok("manifest.json"),
            _ => Err(eyre!("Unsupported source for manifest")),
        }
    }

    fn name() -> &'static str {
        "manifest"
    }
}
