// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::DataSource;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_with::skip_serializing_none;
use std::collections::HashMap;

pub type IlmPolicies = HashMap<String, IlmPolicy>;

#[skip_serializing_none]
#[derive(Serialize, Deserialize)]
pub struct IlmPolicy {
    version: u32,
    modified_date: String,
    policy: Policy,
    in_use_by: InUseBy,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize)]
struct Policy {
    _meta: Option<Box<RawValue>>,
    phases: Phases,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize)]
struct Phases {
    hot: Option<Box<RawValue>>,
    warm: Option<Box<RawValue>>,
    cold: Option<Box<RawValue>>,
    frozen: Option<Box<RawValue>>,
    delete: Option<Box<RawValue>>,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize)]
struct InUseBy {
    indices: Option<Box<RawValue>>,
    data_streams: Option<Box<RawValue>>,
    composable_templates: Option<Box<RawValue>>,
}

impl DataSource for IlmPolicies {
    fn name() -> String {
        "ilm_policies".to_string()
    }
}
