// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::{DataSource, data_source::PathType};
use eyre::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct Version {
    host: String,
    version: String,
    http_address: String,
    pub id: String,
    pub name: String,
    ephemeral_id: String,
    status: String,
    snapshot: bool,
    pipeline: Pipeline,
}

#[derive(Clone, Deserialize, Serialize)]
struct Pipeline {
    workers: u32,
    batch_size: u32,
    batch_delay: u32,
}

impl DataSource for Version {
    fn source(path: PathType, _version: Option<&semver::Version>) -> Result<String> {
        match path {
            PathType::File => Ok("logstash_version.json".to_string()),
            PathType::Url => Ok("/".to_string()),
            PathType::SystemCall => Err(eyre::eyre!(
                "SystemCall path type is not supported for logstash version"
            )),
        }
    }

    fn name() -> String {
        "version".to_string()
    }
}
