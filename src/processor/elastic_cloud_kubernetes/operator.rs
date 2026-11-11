// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! `settings-eck.operator-esdiag`: one summary document per bundle.

use super::{
    DocumentExporter, EckMetadata, ProcessorSummary,
    bundle::{EckLookups, OPERATOR_STREAM},
    logs::ToolLog,
    resources::{BundleVersion, ConfigMap, ServerVersion},
};
use crate::exporter::Exporter;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct Operator {
    eck: EckFields,
    kubernetes: KubernetesFields,
}

#[derive(Serialize)]
struct EckFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    diagnostics: DiagnosticsFields,
    operator: OperatorFields,
    resources: Namespaces,
    #[serde(skip_serializing_if = "Option::is_none")]
    license: Option<License>,
}

#[derive(Serialize)]
struct DiagnosticsFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<String>,
}

#[derive(Serialize)]
struct OperatorFields {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    namespaces: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<Value>,
}

#[derive(Serialize)]
struct Namespaces {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    namespaces: Vec<String>,
}

#[derive(Debug, PartialEq, Serialize)]
struct License {
    #[serde(skip_serializing_if = "Option::is_none")]
    level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enterprise_resource_units: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_managed_memory_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<String>,
}

impl From<BTreeMap<String, String>> for License {
    fn from(mut data: BTreeMap<String, String>) -> Self {
        let number = |value: Option<String>| value.and_then(|value| value.trim().parse::<f64>().ok());
        Self {
            level: data.remove("eck_license_level"),
            enterprise_resource_units: number(data.remove("enterprise_resource_units")),
            total_managed_memory_bytes: number(data.remove("total_managed_memory_bytes")),
            timestamp: data.remove("timestamp"),
        }
    }
}

#[derive(Serialize)]
struct KubernetesFields {
    version: ServerVersion,
}

impl Operator {
    pub fn new(
        eck_version: Option<String>,
        version: BundleVersion,
        tool_log: ToolLog,
        collection_errors: Option<String>,
        config_maps: Vec<ConfigMap>,
    ) -> Self {
        let mut config = None;
        let mut license = None;
        for config_map in config_maps {
            if let Some(yaml) = config_map.operator_config {
                config = config.or_else(|| parse_config(&yaml));
            }
            if let Some(data) = config_map.licensing {
                license = license.or(Some(License::from(data)));
            }
        }
        let diagnostics_version = version
            .diagnostics_version
            .version
            .and_then(|version| version.split_whitespace().next().map(str::to_string));

        Self {
            eck: EckFields {
                version: eck_version,
                diagnostics: DiagnosticsFields {
                    version: diagnostics_version,
                    errors: collection_errors,
                },
                operator: OperatorFields {
                    namespaces: tool_log.operator_namespaces,
                    config,
                },
                resources: Namespaces {
                    namespaces: tool_log.resources_namespaces,
                },
                license,
            },
            kubernetes: KubernetesFields {
                version: version.server_version,
            },
        }
    }
}

/// `eck.yaml` as an object. Anything that is not a YAML mapping is dropped.
fn parse_config(yaml: &str) -> Option<Value> {
    yaml_serde::from_str::<Value>(yaml).ok().filter(Value::is_object)
}

impl DocumentExporter<EckLookups, EckMetadata> for Operator {
    async fn documents_export(self, exporter: &Exporter, _: &EckLookups, metadata: &EckMetadata) -> ProcessorSummary {
        let doc = metadata.document(OPERATOR_STREAM, &self);
        let mut summary = ProcessorSummary::new(OPERATOR_STREAM.to_string());
        match exporter.send(OPERATOR_STREAM.to_string(), vec![doc]).await {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => tracing::error!("Failed to send ECK operator summary: {}", err),
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn licensing_values_are_numeric() {
        let license = License::from(BTreeMap::from([
            ("eck_license_level".to_string(), "enterprise".to_string()),
            ("enterprise_resource_units".to_string(), "2".to_string()),
            ("total_managed_memory_bytes".to_string(), "21474836480".to_string()),
        ]));
        assert_eq!(license.enterprise_resource_units, Some(2.0));
        assert_eq!(license.total_managed_memory_bytes, Some(21_474_836_480.0));
    }

    #[test]
    fn missing_licensing_config_map_omits_license() {
        let operator = Operator::new(None, BundleVersion::default(), ToolLog::default(), None, Vec::new());
        let doc = serde_json::to_value(&operator).unwrap();
        assert!(doc["eck"].get("license").is_none());
        assert!(doc["eck"].get("version").is_none());
    }

    #[test]
    fn operator_config_must_be_a_mapping() {
        assert_eq!(
            parse_config("namespaces: [demo]\nlog-verbosity: 0\n").unwrap()["log-verbosity"],
            0
        );
        assert_eq!(parse_config("just text"), None);
    }
}
