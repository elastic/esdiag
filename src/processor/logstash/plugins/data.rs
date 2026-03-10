// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataSource;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Plugins {
    // Omitted duplicate metadata fields from deserialization
    pub total: u32,
    pub plugins: Vec<Plugin>,
}

#[derive(Deserialize, Serialize)]
pub struct Plugin {
    name: String,
    version: String,
}

impl DataSource for Plugins {
    fn name() -> String {
        "plugins".to_string()
    }

    fn aliases() -> Vec<&'static str> {
        vec!["logstash_plugins"]
    }
}
