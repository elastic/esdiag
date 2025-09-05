// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::data_source::PathType;
use super::super::DataSource;
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct HealthReport {
    // status: String,
    // cluster_name: String,
    pub indicators: HealthIndicators,
}

pub type HealthIndicators = HashMap<String, HealthIndicator>;

#[derive(Serialize, Deserialize)]
pub struct HealthIndicator {
    pub status: String,
    pub symptom: String,
    pub details: Option<Value>,
    #[serde(skip_serializing)]
    pub impacts: Option<Vec<HealthImpact>>,
    #[serde(skip_serializing)]
    pub diagnosis: Option<Vec<HealthDiagnosis>>,
}

#[derive(Serialize, Deserialize)]
pub struct HealthImpact {
    pub id: String,
    pub severity: u32,
    pub description: String,
    pub impact_areas: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct HealthDiagnosis {
    pub id: String,
    pub cause: String,
    pub action: String,
    pub help_url: String,
    pub affected_resources: Value,
}

impl DataSource for HealthReport {
    fn source(kind: PathType) -> Result<&'static str> {
        match kind {
            PathType::File => Ok("internal_health.json"),
            PathType::Url => Ok("_health_report"),
        }
    }

    fn name() -> String {
        "health_report".to_string()
    }
}
