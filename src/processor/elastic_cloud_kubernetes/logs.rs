// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Text sources of an ECK bundle: operator ECS JSON log lines and the
//! `eck-diagnostics.log` run log.

use super::labels::image_tag;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The operator's per-resource name keys and the ECK kind each identifies.
const RESOURCE_NAME_KEYS: [(&str, &str); 10] = [
    ("es_name", "Elasticsearch"),
    ("kb_name", "Kibana"),
    ("kibana_name", "Kibana"),
    ("agent_name", "Agent"),
    ("logstash_name", "Logstash"),
    ("ls_name", "Logstash"),
    ("beat_name", "Beat"),
    ("apm_name", "ApmServer"),
    ("ent_name", "EnterpriseSearch"),
    ("ems_name", "ElasticMapsServer"),
];

/// The allowlisted keys of an operator log line. `error.stack_trace` and the
/// remaining ECS service fields are not read.
#[derive(Deserialize)]
struct RawLine {
    #[serde(rename = "@timestamp")]
    timestamp: Option<String>,
    #[serde(rename = "log.level")]
    level: Option<String>,
    #[serde(rename = "log.logger")]
    logger: Option<String>,
    message: Option<String>,
    namespace: Option<String>,
    iteration: Option<Value>,
    took: Option<Value>,
    error: Option<String>,
    controller: Option<String>,
    es_name: Option<String>,
    kb_name: Option<String>,
    kibana_name: Option<String>,
    agent_name: Option<String>,
    logstash_name: Option<String>,
    ls_name: Option<String>,
    beat_name: Option<String>,
    apm_name: Option<String>,
    ent_name: Option<String>,
    ems_name: Option<String>,
}

impl RawLine {
    fn resource_name(&self, key: &str) -> Option<&String> {
        match key {
            "es_name" => self.es_name.as_ref(),
            "kb_name" => self.kb_name.as_ref(),
            "kibana_name" => self.kibana_name.as_ref(),
            "agent_name" => self.agent_name.as_ref(),
            "logstash_name" => self.logstash_name.as_ref(),
            "ls_name" => self.ls_name.as_ref(),
            "beat_name" => self.beat_name.as_ref(),
            "apm_name" => self.apm_name.as_ref(),
            "ent_name" => self.ent_name.as_ref(),
            "ems_name" => self.ems_name.as_ref(),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub struct OperatorLogLine {
    #[serde(rename = "@timestamp", skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    pub log: LogFields,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub took: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller: Option<String>,
    /// Written to `eck.resource` by the document builder.
    #[serde(skip)]
    pub resource: Option<ResourceRef>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct LogFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ResourceRef {
    pub kind: String,
    pub name: String,
}

/// Parses one operator log line. Returns `None` for anything that is not a
/// JSON object (the `==== START/END logs ====` wrappers, klog output, panics).
pub fn parse_operator_line(line: &str) -> Option<OperatorLogLine> {
    let line = line.trim();
    if !line.starts_with('{') {
        return None;
    }
    let raw: RawLine = serde_json::from_str(line).ok()?;
    let resource = RESOURCE_NAME_KEYS.iter().find_map(|(key, kind)| {
        raw.resource_name(key).map(|name| ResourceRef {
            kind: kind.to_string(),
            name: name.clone(),
        })
    });
    Some(OperatorLogLine {
        log: LogFields {
            level: raw.level,
            logger: raw.logger,
        },
        timestamp: raw.timestamp,
        message: raw.message,
        namespace: raw.namespace,
        iteration: raw.iteration.as_ref().and_then(number).map(|n| n as u64),
        took: raw.took.as_ref().and_then(number),
        error: raw.error,
        controller: raw.controller,
        resource,
    })
}

/// A number, or a string holding one (ECK writes `iteration` as a string).
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
    .filter(|number: &f64| number.is_finite() && *number >= 0.0)
}

/// What ESDiag reads from `eck-diagnostics.log`. The parameter line is a Go
/// `%+v` struct dump, so every field is best-effort.
#[derive(Debug, Default, PartialEq)]
pub struct ToolLog {
    pub eck_version: Option<String>,
    pub operator_namespaces: Vec<String>,
    pub resources_namespaces: Vec<String>,
}

pub fn parse_tool_log(text: &str) -> ToolLog {
    let mut log = ToolLog::default();
    for line in text.lines() {
        if let Some((_, version)) = line.split_once("ECK version is ") {
            log.eck_version = as_version(version.trim()).map(str::to_string).or(log.eck_version);
        }
        if let Some(namespaces) = go_slice(line, "OperatorNamespaces:[") {
            log.operator_namespaces = namespaces;
        }
        if let Some(namespaces) = go_slice(line, "ResourcesNamespaces:[") {
            log.resources_namespaces = namespaces;
        }
    }
    log
}

fn go_slice(line: &str, prefix: &str) -> Option<Vec<String>> {
    let (_, rest) = line.split_once(prefix)?;
    let (items, _) = rest.split_once(']')?;
    Some(items.split_whitespace().map(str::to_string).collect())
}

fn as_version(text: &str) -> Option<&str> {
    let text = text.strip_prefix('v').unwrap_or(text);
    text.starts_with(|c: char| c.is_ascii_digit()).then_some(text)
}

/// The ECK version from the run log, else the first operator image tag that
/// looks like a version.
pub fn eck_version<'a>(tool_log: &ToolLog, operator_images: impl IntoIterator<Item = &'a str>) -> Option<String> {
    tool_log.eck_version.clone().or_else(|| {
        operator_images
            .into_iter()
            .filter_map(image_tag)
            .find_map(as_version)
            .map(str::to_string)
    })
}

/// The `eck-diagnostic-errors.txt` contents, or `None` when the tool recorded
/// no errors (an empty file or an empty Go slice).
pub fn collection_errors(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty() && text != "[]").then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_line_maps_name_key_and_numeric_fields() {
        let line = parse_operator_line(
            r#"{"log.level":"info","@timestamp":"2026-01-15T09:55:02.350Z","log.logger":"elasticsearch-controller","message":"Ending reconciliation run","iteration":"41","namespace":"demo","es_name":"quickstart","took":2.25,"service.version":"3.1.0"}"#,
        )
        .unwrap();
        assert_eq!(line.took, Some(2.25));
        assert_eq!(line.iteration, Some(41));
        assert_eq!(
            line.resource,
            Some(ResourceRef {
                kind: "Elasticsearch".to_string(),
                name: "quickstart".to_string()
            })
        );
        let doc = serde_json::to_value(&line).unwrap();
        assert!(doc.get("service.version").is_none());
    }

    #[test]
    fn short_and_long_name_keys_are_normalized() {
        for (key, kind) in [
            ("kb_name", "Kibana"),
            ("kibana_name", "Kibana"),
            ("ls_name", "Logstash"),
        ] {
            let line = parse_operator_line(&format!(r#"{{"{key}":"x"}}"#)).unwrap();
            assert_eq!(line.resource.unwrap().kind, kind);
        }
    }

    #[test]
    fn error_line_drops_stack_trace() {
        let line = parse_operator_line(
            r#"{"log.level":"error","message":"Reconciler error","error":"boom","error.stack_trace":"frames","controller":"elasticsearch-controller"}"#,
        )
        .unwrap();
        assert_eq!(line.error.as_deref(), Some("boom"));
        assert!(!serde_json::to_string(&line).unwrap().contains("frames"));
    }

    #[test]
    fn non_json_lines_are_rejected() {
        for line in [
            "==== START logs for elastic-system/elastic-operator-0 ====",
            "I0115 09:55:04.000000       1 leaderelection.go:250] attempting to acquire leader lease",
            "{not json",
            "[1, 2]",
            "",
        ] {
            assert_eq!(parse_operator_line(line), None, "{line}");
        }
    }

    #[test]
    fn tool_log_yields_version_and_namespaces() {
        let log = parse_tool_log(
            "2026/01/15 10:00:00 ECK diagnostics with parameters: {ECKVersion: OperatorNamespaces:[elastic-system] ResourcesNamespaces:[demo other] OutputDir:}\n2026/01/15 10:00:02 ECK version is 3.1.0\n",
        );
        assert_eq!(
            log,
            ToolLog {
                eck_version: Some("3.1.0".to_string()),
                operator_namespaces: vec!["elastic-system".to_string()],
                resources_namespaces: vec!["demo".to_string(), "other".to_string()],
            }
        );
    }

    #[test]
    fn version_falls_back_to_operator_image_tag() {
        let log = parse_tool_log("2026/01/15 10:00:02 Extracting Kubernetes diagnostics from demo\n");
        assert_eq!(log.eck_version, None);
        assert_eq!(
            eck_version(
                &log,
                ["registry/eck/eck-operator", "docker.elastic.co/eck/eck-operator:3.1.0"]
            ),
            Some("3.1.0".to_string())
        );
        assert_eq!(eck_version(&log, ["eck-operator:latest"]), None);
    }

    #[test]
    fn empty_collection_errors_are_absent() {
        assert_eq!(collection_errors(" []\n"), None);
        assert_eq!(collection_errors(""), None);
        assert_eq!(collection_errors("[boom]\n"), Some("[boom]".to_string()));
    }
}
