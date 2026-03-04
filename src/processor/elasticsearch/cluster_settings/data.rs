// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::DataSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize)]
pub struct ClusterSettings {
    #[serde(default)]
    pub transient: Value,
    #[serde(default)]
    pub persistent: Value,
}

#[derive(Serialize, Deserialize)]
pub struct ClusterSettingsDefaults {
    #[serde(default)]
    pub transient: Value,
    #[serde(default)]
    pub persistent: Value,
    #[serde(default)]
    pub defaults: Value,
}

impl ClusterSettings {
    pub fn get_display_name(&self) -> Option<String> {
        self.persistent
            .get("cluster.metadata.display_name")
            .and_then(|name| name.as_str().map(|s| s.to_string()))
    }
}

impl ClusterSettingsDefaults {
    pub fn get_display_name(&self) -> Option<String> {
        if let Some(display_name) = self.persistent.get("cluster.metadata.display_name") {
            return display_name.as_str().map(|s| s.to_string());
        }

        let (Some(name), Some(project_type)) = (
            self.defaults.get("serverless.project_id"),
            self.defaults.get("serverless.project_type"),
        ) else {
            return None;
        };

        let name = name.as_str()?.chars().take(8).collect::<String>();
        let project_type = project_type.as_str()?.trim_start_matches("elasticsearch_");
        Some(format!("{project_type}-{name}"))
    }
}

impl DataSource for ClusterSettings {
    fn name() -> String {
        "cluster_settings".to_string()
    }
}

impl DataSource for ClusterSettingsDefaults {
    fn name() -> String {
        "cluster_settings_defaults".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cluster_settings_parses_without_defaults() {
        let json = r#"{
            "persistent": {"cluster.metadata.display_name": "prod-cluster"},
            "transient": {}
        }"#;
        let settings: ClusterSettings = serde_json::from_str(json).expect("cluster settings parse");
        assert_eq!(settings.get_display_name().as_deref(), Some("prod-cluster"));
    }

    #[test]
    fn cluster_settings_defaults_uses_serverless_defaults_for_display_name() {
        let json = r#"{
            "persistent": {},
            "transient": {},
            "defaults": {
                "serverless.project_id": "1234567890abcdef",
                "serverless.project_type": "elasticsearch_search"
            }
        }"#;
        let settings: ClusterSettingsDefaults =
            serde_json::from_str(json).expect("cluster settings defaults parse");
        assert_eq!(
            settings.get_display_name().as_deref(),
            Some("search-12345678")
        );
    }
}
