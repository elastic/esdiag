// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::DataSource;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

#[skip_serializing_none]
#[derive(Deserialize)]
pub struct Licenses {
    pub license: License,
}

#[skip_serializing_none]
#[derive(Clone, Deserialize, Serialize)]
pub struct License {
    status: String,
    uid: String,
    r#type: String,
    //issue_date: String,
    issue_date_in_millis: u64,
    //expiry_date: String,
    expiry_date_in_millis: u64,
    max_nodes: Option<i32>,
    max_resource_units: Option<i32>,
    issued_to: String,
    issuer: Option<String>,
    start_date_in_millis: i64,
}

impl DataSource for Licenses {

    fn name() -> String {
        "licenses".to_string()
    }
}
