// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::DataSource;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_with::skip_serializing_none;
use std::collections::HashMap;

pub type SlmPolicies = HashMap<String, SlmPolicy>;

#[skip_serializing_none]
#[derive(Serialize, Deserialize)]
pub struct SlmPolicy {
    version: u32,
    // modified_date: String,
    modified_date_millis: u64,
    policy: Box<RawValue>,
    last_success: Option<Box<RawValue>>,
    last_failure: Option<Box<RawValue>>,
    // next_execution: Option<String>,
    next_execution_millis: Option<u64>,
    stats: Option<Box<RawValue>>,
}

impl DataSource for SlmPolicies {
    fn name() -> String {
        "slm_policies".to_string()
    }
}
