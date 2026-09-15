// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Serialize)]
pub struct Agents(pub Value);

impl DataSource for Agents {
    fn name() -> String {
        "kibana_fleet_agents".to_string()
    }
}

#[derive(Deserialize, Serialize)]
pub struct AgentPolicies(pub Value);

impl DataSource for AgentPolicies {
    fn name() -> String {
        "kibana_fleet_agent_policies".to_string()
    }
}

#[derive(Deserialize, Serialize)]
pub struct Packages(pub Value);

impl DataSource for Packages {
    fn name() -> String {
        "kibana_fleet_packages".to_string()
    }
}

#[derive(Deserialize, Serialize)]
pub struct AgentStatus(pub Value);

impl DataSource for AgentStatus {
    fn name() -> String {
        "kibana_fleet_agent_status".to_string()
    }
}
