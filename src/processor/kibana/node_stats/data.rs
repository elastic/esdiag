// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataSource;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Deserialize, Serialize)]
pub struct NodeStats {
    pub process: Value,
    pub os: Value,
    /// Version-dependent stats such as `processes`, `concurrent_connections`, `requests`,
    /// and `elasticsearch_client`.
    #[serde(flatten)]
    pub stats: Map<String, Value>,
}

impl DataSource for NodeStats {
    fn name() -> String {
        "kibana_stats".to_string()
    }
}
