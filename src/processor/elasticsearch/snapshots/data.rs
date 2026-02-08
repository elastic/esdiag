// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::data_source::PathType;
use super::super::DataSource;
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub type SnapshotRepositories = HashMap<String, Value>;

#[derive(Serialize, Deserialize)]
pub struct Snapshots {
    pub snapshots: Vec<Value>,
}

impl DataSource for SnapshotRepositories {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("commercial/snapshot_repositories.json"),
            PathType::Url => Ok("_snapshot"),
        }
    }

    fn name() -> String {
        "snapshot_repositories".to_string()
    }
}

impl DataSource for Snapshots {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("commercial/snapshots.json"),
            PathType::Url => Ok("_snapshot/_all/_all"),
        }
    }

    fn name() -> String {
        "snapshots".to_string()
    }
}
