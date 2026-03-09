// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataSource;
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct Version {
    host: String,
    pub version: String,
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
    fn name() -> String {
        "version".to_string()
    }

    fn aliases() -> Vec<&'static str> {
        vec!["logstash_version"]
    }

    fn product() -> &'static str {
        "logstash"
    }
}
