// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::elasticsearch;
use super::DiagPath;
use crate::data::{Application, Platform};
use eyre::Result;
use serde::{Deserialize, Deserializer, Serialize};
use std::str::FromStr;

/// Legacy single-axis product classification used only by diagnostic manifest
/// wire formats.
///
/// Do not use this type for runtime dispatch or saved-host classification.
/// Those domains use [`Application`] and [`Platform`] independently.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(clippy::upper_case_acronyms)]
pub(super) enum ManifestProduct {
    Agent,
    ECE,
    ECK,
    ElasticCloudHosted,
    #[default]
    Elasticsearch,
    Kibana,
    KubernetesPlatform,
    Logstash,
    Unknown,
}

impl ManifestProduct {
    pub(super) fn split(&self) -> (Platform, Option<Application>) {
        match self {
            Self::Agent => (Platform::Unknown, Some(Application::Agent)),
            Self::Elasticsearch => (Platform::Unknown, Some(Application::Elasticsearch)),
            Self::Kibana => (Platform::Unknown, Some(Application::Kibana)),
            Self::Logstash => (Platform::Unknown, Some(Application::Logstash)),
            Self::ECE => (Platform::ECE, None),
            Self::ECK => (Platform::ECK, None),
            Self::ElasticCloudHosted => (Platform::ElasticCloudHosted, None),
            Self::KubernetesPlatform => (Platform::KubernetesPlatform, None),
            Self::Unknown => (Platform::Unknown, None),
        }
    }

    pub(super) fn application(&self) -> Option<Application> {
        self.split().1
    }

    pub(super) fn from_classification(platform: Platform, application: Option<Application>) -> Self {
        match application {
            Some(Application::Elasticsearch) => Self::Elasticsearch,
            Some(Application::Kibana) => Self::Kibana,
            Some(Application::Logstash) => Self::Logstash,
            Some(Application::Agent) => Self::Agent,
            None => match platform {
                Platform::ECE => Self::ECE,
                Platform::ECK => Self::ECK,
                Platform::ElasticCloudHosted => Self::ElasticCloudHosted,
                Platform::KubernetesPlatform => Self::KubernetesPlatform,
                Platform::SelfManaged | Platform::Unknown => Self::Unknown,
            },
        }
    }
}

impl std::fmt::Display for ManifestProduct {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Agent => write!(fmt, "Agent"),
            Self::ECE => write!(fmt, "ECE"),
            Self::ECK => write!(fmt, "ECK"),
            Self::ElasticCloudHosted => write!(fmt, "ElasticCloudHosted"),
            Self::Elasticsearch => write!(fmt, "Elasticsearch"),
            Self::Kibana => write!(fmt, "Kibana"),
            Self::KubernetesPlatform => write!(fmt, "KubernetesPlatform"),
            Self::Logstash => write!(fmt, "Logstash"),
            Self::Unknown => write!(fmt, "Unknown"),
        }
    }
}

impl FromStr for ManifestProduct {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_lowercase().as_str() {
            "agent" => Ok(Self::Agent),
            "ece" => Ok(Self::ECE),
            "eck" => Ok(Self::ECK),
            "hosted" | "elastic-cloud-hosted" | "elasticcloudhosted" => Ok(Self::ElasticCloudHosted),
            "es" | "elasticsearch" => Ok(Self::Elasticsearch),
            "kb" | "kibana" => Ok(Self::Kibana),
            "ls" | "logstash" => Ok(Self::Logstash),
            "mki" | "kubernetesplatform" => Ok(Self::KubernetesPlatform),
            "unknown" => Ok(Self::Unknown),
            _ => Err("Unknown product".to_string()),
        }
    }
}

impl<'de> Deserialize<'de> for ManifestProduct {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: String = Deserialize::deserialize(deserializer)?;
        Self::from_str(&value).map_err(|error| serde::de::Error::custom(format!("Unknown product: {error}")))
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub diag_type: Option<String>,
    pub diagnostic_inputs: Option<String>,
    pub diag_version: Option<String>,
    #[serde(default)]
    pub(super) product: ManifestProduct,
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
    product: ManifestProduct,
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
            product: ManifestProduct::Elasticsearch,
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
            product: ManifestProduct::Elasticsearch,
            product_version: Some(ProductVersion::from(cluster.version)),
            runner: None,
            collection_date: chrono::Utc::now().to_rfc3339(),
            included_diagnostics: None,
        })
    }
}

impl Manifest {
    pub const FILENAME: &'static str = "manifest.json";
}

#[cfg(test)]
mod tests {
    use super::Manifest;

    #[test]
    fn legacy_manifest_product_field_keeps_its_string_wire_shape() {
        for product in [
            "agent",
            "ece",
            "eck",
            "elasticcloudhosted",
            "elasticsearch",
            "kibana",
            "kubernetesplatform",
            "logstash",
            "unknown",
        ] {
            let manifest: Manifest = serde_json::from_value(serde_json::json!({
                "product": product,
                "collectionDate": "2026-04-25T20:52:09.948Z"
            }))
            .unwrap_or_else(|error| panic!("historical product {product} should deserialize: {error}"));

            let serialized = serde_json::to_value(&manifest).expect("serialize manifest");
            assert_eq!(
                serialized["product"], product,
                "product wire value changed for {product}"
            );
        }
    }
}
