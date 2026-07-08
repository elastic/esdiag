// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::DataSource;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::HashMap;

// The collector does not wire this API yet; keep the model close to its processor
// until searchable snapshot stats collection is enabled.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize)]
pub struct SearchableSnapshotsStats {
    pub _shards: Box<RawValue>,
    pub total: Vec<Box<RawValue>>,
    pub indices: HashMap<String, Total>,
}

// Kept with SearchableSnapshotsStats so the dormant model mirrors the API shape.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize)]
pub struct Total {
    pub total: Vec<Box<RawValue>>,
}

impl DataSource for SearchableSnapshotsStats {
    fn name() -> String {
        "searchable_snapshots_stats".to_string()
    }
}
