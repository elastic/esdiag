// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::processor::{DataSource, PathType};
use eyre::Result;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct PendingTasks {
    pub tasks: Vec<PendingTask>,
}

#[derive(Deserialize, Serialize)]
pub struct PendingTask {
    insert_order: u64,
    priority: String,
    source: String,
    executing: bool,
    time_in_queue_millis: i64,
}

impl DataSource for PendingTasks {
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
        "tasks".to_string()
    }
}
