// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::elasticsearch;
use super::{DataSource, DiagPath, data_source::PathType};
use crate::data::Product;
use eyre::{Result, eyre};
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
    /// Kubernetes diagnostic bundles can contain multiple stack diagnostics
    pub included_diagnostics: Option<Vec<DiagPath>>,
}

pub struct ManifestBuilder {
    diag_type: Option<String>,
    diagnostic_inputs: Option<String>,
    diag_version: Option<String>,
    product: Product,
    product_version: Option<ProductVersion>,
    runner: Option<String>,
    collection_date: String,
    included_diagnostics: Option<Vec<DiagPath>>,
}

impl Default for ManifestBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ManifestBuilder {
    pub fn new() -> Self {
        Self {
            diag_type: None,
            diagnostic_inputs: None,
            diag_version: None,
            product: Product::Elasticsearch,
            product_version: None,
            runner: None,
            collection_date: chrono::Utc::now().to_rfc3339(),
            included_diagnostics: None,
        }
    }

    pub fn build(self) -> Manifest {
        Manifest {
            diag_type: self.diag_type,
            diagnostic_inputs: self.diagnostic_inputs,
            diag_version: self.diag_version,
            product: self.product,
            product_version: self.product_version,
            runner: self.runner,
            collection_date: self.collection_date,
            included_diagnostics: self.included_diagnostics,
        }
    }

    /// The runner used to execute the diagnostic
    pub fn runner(mut self, runner: &str) -> Self {
        self.runner = Some(runner.to_string());
        self
    }

    /// A collection date, used if the manifest does not have one
    pub fn collection_date(mut self, date: String) -> Self {
        self.collection_date = date;
        self
    }
}

impl From<elasticsearch::Cluster> for ManifestBuilder {
    fn from(version: elasticsearch::Cluster) -> Self {
        let builder = ManifestBuilder::new();
        Self {
            diag_type: Some("es-unknown".to_string()),
            product_version: Some(ProductVersion::from(version.version)),
            runner: Some("unknown".to_string()),
            ..builder
        }
    }
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
    type Error = eyre::Error;

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
    fn source(path: PathType, _version: Option<&semver::Version>) -> Result<String> {
        match path {
            PathType::File => Ok("manifest.json".to_string()),
            _ => Err(eyre!("Unsupported source for manifest")),
        }
    }

    fn name() -> String {
        "manifest".to_string()
    }
}
