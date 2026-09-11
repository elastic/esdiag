// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataSource;
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct Version {
    pub name: String,
    #[serde(rename(deserialize = "uuid", serialize = "id"))]
    pub id: String,
    pub version: VersionDetails,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct VersionDetails {
    pub number: String,
    pub build_hash: String,
    pub build_number: u32,
    pub build_snapshot: bool,
}

impl DataSource for Version {
    fn name() -> String {
        "kibana_status".to_string()
    }
}
