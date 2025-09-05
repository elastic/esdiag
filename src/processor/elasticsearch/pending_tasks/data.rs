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
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("cluster_pending_tasks.json"),
            PathType::Url => Ok("_cluster/pending_tasks"),
        }
    }

    fn name() -> String {
        "tasks".to_string()
    }
}
