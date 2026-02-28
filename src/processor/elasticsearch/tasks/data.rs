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

#[derive(Debug, Deserialize, Serialize)]
pub struct Tasks {
    pub nodes: HashMap<String, NodeTasks>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct NodeTasks {
    pub tasks: HashMap<String, Task>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Task {
    pub action: String,
    cancellable: bool,
    cancelled: Option<bool>,
    pub description: Option<String>,
    headers: Option<Box<RawValue>>,
    pub id: u64,
    //node: Option<String>, // omitted in favor of enriched node field
    #[serde(skip_serializing)] // skipped in favor of subobject field
    pub parent_task_id: Option<String>,
    #[serde(skip_deserializing)] // not in original data
    parent_task: Option<ParentTask>,
    r#type: String,
    running_time_in_nanos: u64,
    start_time_in_millis: u64,
    status: Option<Box<RawValue>>,
}

impl DataSource for Tasks {
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

#[derive(Debug, Clone, Serialize)]
pub struct ParentTask {
    id: u64,
    node: String,
}

impl From<String> for ParentTask {
    fn from(parent_task: String) -> Self {
        let mut parts = parent_task.split(':');
        ParentTask {
            node: parts.next().unwrap_or_default().to_string(),
            id: parts.next().unwrap_or_default().parse().unwrap_or_default(),
        }
    }
}
