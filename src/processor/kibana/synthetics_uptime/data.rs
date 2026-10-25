// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Serialize)]
pub struct SyntheticsFilters(pub Value);

impl DataSource for SyntheticsFilters {
    fn name() -> String {
        "kibana_synthetics_monitor_filters".to_string()
    }
}

#[derive(Deserialize, Serialize)]
pub struct UptimeLocations(pub Value);

impl DataSource for UptimeLocations {
    fn name() -> String {
        "kibana_uptime_locations".to_string()
    }
}
