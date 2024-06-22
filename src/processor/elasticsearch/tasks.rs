use super::lookup::node::NodeData;
use super::metadata::{DataStream, Metadata, MetadataDoc};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn enrich(metadata: &Metadata, data: String) -> Vec<Value> {
    let data = match serde_json::from_str::<Nodes>(&data) {
        Ok(data) => data,
        Err(e) => {
            log::error!("Failed to deserialize tasks: {}", e);
            return Vec::new();
        }
    };
    let lookup = &metadata.lookup;
    let metadata = &metadata.as_doc;
    let nodes: Vec<(_, _)> = data.nodes.into_iter().collect();

    let task_doc = TaskDoc::new(metadata.clone(), DataStream::from("metrics-task-esdiag"));

    let tasks: Vec<Value> = nodes
        .into_par_iter()
        .flat_map(|(node_id, node)| {
            node.tasks
                .iter()
                .collect::<Vec<_>>()
                .into_par_iter()
                .map(|(_, task)| {
                    let node = lookup.node.by_id(node_id.as_str()).cloned();
                    let task_doc = task_doc.clone().with(node, task.clone());
                    json!(task_doc)
                })
                .collect::<Vec<Value>>()
        })
        .collect();

    log::debug!("task docs: {}", tasks.len());
    tasks
}

// Serializing data structures

#[derive(Clone, Serialize)]
pub struct TaskDoc {
    #[serde(flatten)]
    metadata: MetadataDoc,
    data_stream: DataStream,
    node: Option<NodeData>,
    task: Option<TaskData>,
}

impl TaskDoc {
    pub fn new(metadata: MetadataDoc, data_stream: DataStream) -> Self {
        TaskDoc {
            data_stream,
            metadata,
            node: None,
            task: None,
        }
    }

    pub fn with(mut self, node: Option<NodeData>, task: Task) -> Self {
        self.node = node;
        self.task = Some(TaskData::from(task));
        self
    }
}

#[derive(Clone, Serialize)]
pub struct TaskData {
    id: u64,
    r#type: String,
    action: String,
    description: String,
    start_time_in_millis: u64,
    running_time_in_nanos: u64,
    cancellable: bool,
    cancelled: Option<bool>,
    parent_task: Option<ParentTask>,
    headers: Option<Value>,
}

impl TaskData {
    pub fn from(task: Task) -> Self {
        TaskData {
            id: task.id,
            r#type: task.r#type,
            action: task.action,
            description: task.description,
            start_time_in_millis: task.start_time_in_millis,
            running_time_in_nanos: task.running_time_in_nanos,
            cancellable: task.cancellable,
            cancelled: task.cancelled,
            parent_task: task.parent_task_id.as_ref().map(|id| ParentTask::from(id)),
            headers: task.headers.clone(),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct ParentTask {
    id: u64,
    node: String,
}

impl ParentTask {
    pub fn from(parent_task: &String) -> Self {
        let mut parts = parent_task.split(':');
        ParentTask {
            id: parts.next().unwrap_or_default().parse().unwrap_or_default(),
            node: parts.next().unwrap_or_default().to_string(),
        }
    }
}

// Deserializing data structures

#[derive(Debug, Deserialize)]
struct Nodes {
    nodes: HashMap<String, Node>,
}

#[derive(Debug, Deserialize)]
struct Node {
    tasks: HashMap<String, Task>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Task {
    id: u64,
    r#type: String,
    action: String,
    description: String,
    start_time_in_millis: u64,
    running_time_in_nanos: u64,
    cancellable: bool,
    cancelled: Option<bool>,
    parent_task_id: Option<String>,
    headers: Option<Value>,
}
