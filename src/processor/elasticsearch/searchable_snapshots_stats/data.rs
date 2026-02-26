// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::data_source::PathType;
use super::super::DataSource;
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize)]
pub struct SearchableSnapshotsStats {
    pub _shards: Box<RawValue>,
    pub total: Vec<Box<RawValue>>,
    pub indices: HashMap<String, Total>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Total {
    pub total: Vec<Box<RawValue>>,
}

impl DataSource for SearchableSnapshotsStats {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("commercial/searchable_snapshots_stats.json"),
            PathType::Url => Ok("_searchable_snapshots/stats"),
        }
    }

    fn name() -> String {
        "searchable_snapshots_stats".to_string()
    }
}
