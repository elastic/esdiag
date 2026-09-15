// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Serialize)]
pub struct DetectionEngineHealth(pub Value);

impl DataSource for DetectionEngineHealth {
    fn name() -> String {
        "kibana_detection_engine_health_cluster".to_string()
    }
}

#[derive(Deserialize, Serialize)]
pub struct DetectionEngineRules(pub Value);

impl DataSource for DetectionEngineRules {
    fn name() -> String {
        "kibana_detection_engine_rules_installed".to_string()
    }
}
