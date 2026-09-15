// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Serialize)]
pub struct Roles(pub Value);

impl DataSource for Roles {
    fn name() -> String {
        "kibana_roles".to_string()
    }
}

#[derive(Deserialize, Serialize)]
pub struct Users(pub Value);

impl DataSource for Users {
    fn name() -> String {
        "kibana_user".to_string()
    }
}

#[derive(Deserialize, Serialize)]
pub struct Actions(pub Value);

impl DataSource for Actions {
    fn name() -> String {
        "kibana_actions".to_string()
    }
}
