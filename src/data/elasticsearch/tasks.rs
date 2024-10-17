use crate::data::{diagnostic::data_source::DataSource, Uri};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct Tasks {
    pub nodes: HashMap<String, NodeTasks>,
}

#[derive(Debug, Deserialize)]
pub struct NodeTasks {
    pub tasks: HashMap<String, Task>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Task {
    action: String,
    cancellable: bool,
    cancelled: Option<bool>,
    description: String,
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
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("tasks.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_tasks"),
            _ => Err(eyre!("Unsupported source for tasks")),
        }
    }

    fn name() -> &'static str {
        "tasks"
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
