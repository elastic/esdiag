// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Serialize)]
pub struct Spaces(pub Value);

impl DataSource for Spaces {
    fn name() -> String {
        "kibana_spaces".to_string()
    }
}
