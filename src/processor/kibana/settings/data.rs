// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Serialize)]
pub struct FleetSettings(pub Value);

impl DataSource for FleetSettings {
    fn name() -> String {
        "kibana_fleet_settings".to_string()
    }
}

#[derive(Deserialize, Serialize)]
pub struct UptimeSettings(pub Value);

impl DataSource for UptimeSettings {
    fn name() -> String {
        "kibana_uptime_settings".to_string()
    }
}
