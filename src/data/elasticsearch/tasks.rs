use crate::data::diagnostic::{DataSource, data_source::PathType, elasticsearch::DataSet};
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    headers: Option<Value>,
    id: u64,
    //node: Option<String>, // omitted in favor of enriched node field
    #[serde(skip_serializing)] // skipped in favor of subobject field
    pub parent_task_id: Option<String>,
    #[serde(skip_deserializing)] // not in original data
    parent_task: Option<ParentTask>,
    r#type: String,
    running_time_in_nanos: u64,
    start_time_in_millis: u64,
    status: Option<Value>,
}

impl DataSource for Tasks {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("tasks.json"),
            PathType::Url => Ok("_tasks?detailed=true"),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::Tasks)
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
            id: parts.next().unwrap_or_default().parse().unwrap_or_default(),
            node: parts.next().unwrap_or_default().to_string(),
        }
    }
}
