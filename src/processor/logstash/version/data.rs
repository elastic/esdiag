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
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("logstash_version.json"),
            PathType::Url => Ok("/"),
        }
    }

    fn name() -> String {
        "version".to_string()
    }
}
