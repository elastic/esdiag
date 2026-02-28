// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::data_source::PathType;
use super::super::DataSource;
use eyre::Result;
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
    fn source(path: PathType, version: Option<&semver::Version>) -> Result<String> {
        let name = Self::name();
        if let Ok(source_conf) =
            crate::processor::diagnostic::data_source::get_source(Self::product(), &name)
        {
            match path {
                PathType::File => Ok(source_conf.get_file_path(&name)),
                PathType::Url => {
                    let v = version.ok_or_else(|| eyre::eyre!("Version required for URL"))?;
                    source_conf.get_url(v)
                }
            }
        } else {
            // Fallback for missing or not-yet-supported sources
            eyre::bail!(
                "Source configuration missing for product: {}, name: {}",
                Self::product(),
                name
            )
        }
    }

    fn name() -> String {
        "slm_policies".to_string()
    }
}
