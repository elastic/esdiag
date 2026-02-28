// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::data_source::PathType;
use super::super::DataSource;
use eyre::Result;
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
        "licenses".to_string()
    }
}
