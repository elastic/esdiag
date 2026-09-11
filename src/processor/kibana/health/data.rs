// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Serialize)]
pub struct TaskManagerHealth(pub Value);

impl DataSource for TaskManagerHealth {
    fn name() -> String {
        "kibana_task_manager_health".to_string()
    }
}

#[derive(Deserialize, Serialize)]
pub struct StackMonitoringHealth(pub Value);

impl DataSource for StackMonitoringHealth {
    fn name() -> String {
        "kibana_stack_monitoring_health".to_string()
    }
}
