// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::data_source::PathType;
use super::super::DataSource;
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    _meta: Option<Value>,
    phases: Phases,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize)]
struct Phases {
    hot: Option<Value>,
    warm: Option<Value>,
    cold: Option<Value>,
    frozen: Option<Value>,
    delete: Option<Value>,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize)]
struct InUseBy {
    indices: Vec<String>,
    data_streams: Option<Vec<String>>,
    composable_templates: Option<Vec<String>>,
}

impl DataSource for IlmPolicies {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("commercial/ilm_policies.json"),
            PathType::Url => Ok("_ilm/policy"),
        }
    }

    fn name() -> String {
        "ilm_policies".to_string()
    }
}
