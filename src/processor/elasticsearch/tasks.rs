use super::{DataProcessor, ElasticsearchMetadata, Lookups};
use crate::{
    data::elasticsearch::{NodeTasks, ParentTask, Task, Tasks},
    processor::{lookup::elasticsearch::node::NodeSummary, Metadata},
};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

impl DataProcessor<ElasticsearchMetadata> for Tasks {
    fn generate_docs(
        self,
        lookups: Arc<Lookups>,
        metadata: Arc<ElasticsearchMetadata>,
    ) -> (String, Vec<Value>) {
        log::debug!("processing tasks");
        let data_stream = "metrics-task-esdiag".to_string();
        let lookup_node = &lookups.node;
        let task_metadata = metadata.for_data_stream(&data_stream).as_meta_doc();

        let nodes: Vec<(String, NodeTasks)> = self.nodes.into_par_iter().collect();

        let tasks: Vec<Value> = nodes
            .into_par_iter()
            .flat_map(|(node_id, node)| {
                node.tasks
                    .iter()
                    .collect::<Vec<_>>()
                    .into_par_iter()
                    .map(|(_, task)| {
                        let node = lookup_node
                            .by_id(node_id.as_str())
                            .cloned()
                            .expect("Node not found for task");
                        serde_json::to_value(TaskDoc::new(task, task_metadata.clone(), node))
                            .unwrap_or_default()
                    })
                    .collect::<Vec<Value>>()
            })
            .collect();

        log::debug!("task docs: {}", tasks.len());
        (data_stream, tasks)
    }
}

#[derive(Clone, Serialize)]
pub struct TaskDoc {
    #[serde(flatten)]
    metadata: Value,
    node: NodeSummary,
    task: TaskWithParent,
}

impl TaskDoc {
    pub fn new(task: &Task, metadata: Value, node: NodeSummary) -> Self {
        let parent = task
            .parent_task_id
            .as_ref()
            .map(|id| ParentTask::from(id.clone()));
        TaskDoc {
            metadata: metadata.clone(),
            node,
            task: TaskWithParent {
                task: task.clone(),
                parent,
            },
        }
    }
}

#[derive(Clone, Serialize)]
pub struct TaskWithParent {
    #[serde(flatten)]
    task: Task,
    parent: Option<ParentTask>,
}
