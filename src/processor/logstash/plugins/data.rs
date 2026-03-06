// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::{DataSource, data_source::PathType};
use eyre::Result;
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
    fn source(path: PathType, _version: Option<&semver::Version>) -> Result<String> {
        match path {
            PathType::File => Ok("logstash_plugins.json".to_string()),
            PathType::Url => Ok("_node/plugins".to_string()),
            PathType::SystemCall => Err(eyre::eyre!(
                "SystemCall path type is not supported for logstash plugins"
            )),
        }
    }

    fn name() -> String {
        "plugins".to_string()
    }
}
