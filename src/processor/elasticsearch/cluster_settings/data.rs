// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::DataSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize)]
pub struct ClusterSettings {
    pub transient: Value,
    pub persistent: Value,
    pub defaults: Value,
}

impl ClusterSettings {
    pub fn get_display_name(&self) -> Option<String> {
        if let Some(display_name) = self.persistent.get("cluster.metadata.display_name") {
            Some(display_name.as_str().unwrap().to_string())
        } else if let (Some(name), Some(project_type)) = (
            self.defaults.get("serverless.project_id"),
            self.defaults.get("serverless.project_type"),
        ) {
            let name = name.as_str().unwrap()[..8].to_string();
            let project_type = project_type
                .as_str()
                .unwrap()
                .trim_start_matches("elasticsearch_");
            Some(format!("{project_type}-{name}"))
        } else {
            None
        }
    }
}

impl DataSource for ClusterSettings {
    fn name() -> String {
        "cluster_settings".to_string()
    }
}
