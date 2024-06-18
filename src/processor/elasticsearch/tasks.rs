use super::lookup::node::NodeData;
use super::metadata::{DataStream, Metadata, MetadataDoc};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{json, Value};

pub fn enrich(metadata: &Metadata, data: Value) -> Vec<Value> {
    let lookup = &metadata.lookup;
    let metadata = &metadata.as_doc;
    let nodes: Vec<_> = match data["nodes"].as_object().take() {
        Some(data) => data.iter().collect(),
        None => {
            log::error!("Failed to deserialize tasks");
            return Vec::new();
        }
    };

    let task_doc = TaskDoc::new(metadata.clone(), DataStream::from("metrics-task-esdiag"));

    let tasks: Vec<Value> = nodes
        .into_par_iter()
        .flat_map(|(node_id, node)| {
            let tasks = match node["tasks"].as_object().take() {
                Some(data) => data,
                None => return Vec::new(),
            };
            tasks
                .iter()
                .collect::<Vec<_>>()
                .into_par_iter()
                .map(|(_, task)| {
                    let node = lookup.node.by_id(node_id.as_str()).cloned();
                    let task_doc = task_doc.clone().with(node, task.clone());
                    json!(task_doc)
                })
                .collect()
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
    task: Value,
}

impl TaskDoc {
    pub fn new(metadata: MetadataDoc, data_stream: DataStream) -> Self {
        TaskDoc {
            data_stream,
            metadata,
            node: None,
            task: Value::Null,
        }
    }

    pub fn with(mut self, node: Option<NodeData>, task: Value) -> Self {
        self.node = node;
        self.task = task;
        self
    }
}
