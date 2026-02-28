// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{
    DocumentExporter, ElasticsearchMetadata, Lookups, ProcessorSummary, metadata::MetadataRawValue,
    nodes::NodeDocument,
};
use super::{NodeTasks, ParentTask, Task, Tasks};
use crate::exporter::Exporter;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use serde_with::skip_serializing_none;

impl DocumentExporter<Lookups, ElasticsearchMetadata> for Tasks {
    async fn documents_export(
        self,
        exporter: &Exporter,
        lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        log::debug!("processing tasks");
        let data_stream = "metrics-task-esdiag".to_string();
        let lookup_node = &lookups.node;
        let task_metadata = metadata.for_data_stream(&data_stream);

        let mut nodes: Vec<(String, NodeTasks)> = self.nodes.into_par_iter().collect();

        let tasks: Vec<Value> = nodes
            .par_drain(..)
            .flat_map(|(node_id, mut node)| {
                node.tasks
                    .par_drain()
                    .map(|(_, task)| {
                        let node = lookup_node.by_id(node_id.as_str()).cloned();
                        if node.is_none() {
                            log::warn!("Node [{}] not found for task [{}]", node_id, task.id);
                        }
                        serde_json::to_value(EnrichedTask::new(task, task_metadata.clone(), node))
                            .unwrap_or_default()
                    })
                    .collect::<Vec<Value>>()
            })
            .collect();

        log::debug!("task docs: {}", tasks.len());
        let mut summary = ProcessorSummary::new(data_stream.clone());
        match exporter.send(data_stream, tasks).await {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => log::error!("Failed to send tasks: {}", err),
        }
        summary
    }
}

#[derive(Clone, Serialize)]
pub struct EnrichedTask {
    #[serde(flatten)]
    metadata: MetadataRawValue,
    node: Option<NodeDocument>,
    task: TaskWithParent,
    #[serde(flatten)]
    data: Option<TaskData>,
}

impl EnrichedTask {
    pub fn new(task: Task, metadata: MetadataRawValue, node: Option<NodeDocument>) -> Self {
        let parent = task
            .parent_task_id
            .as_ref()
            .map(|id| ParentTask::from(id.clone()));
        EnrichedTask {
            metadata,
            node,
            data: TaskData::new((task.action.clone(), task.description.clone())),
            task: TaskWithParent { task, parent },
        }
    }
}

#[skip_serializing_none]
#[derive(Clone, Serialize)]
pub struct TaskData {
    shard: Option<TaskShard>,
    index: Option<TaskIndex>,
}

#[skip_serializing_none]
#[derive(Clone, Serialize)]
pub struct TaskShard {
    number: Option<u32>,
    primary: Option<bool>,
    bulk: Option<BulkDocs>,
}

#[skip_serializing_none]
#[derive(Clone, Serialize)]
pub struct TaskIndex {
    count: Option<u32>,
    name: Option<Vec<String>>,
    bulk: Option<BulkDocs>,
}

#[skip_serializing_none]
#[derive(Clone, Serialize)]
pub struct BulkDocs {
    docs: Option<u64>,
}

#[skip_serializing_none]
#[derive(Clone, Serialize)]
pub struct TaskWithParent {
    #[serde(flatten)]
    task: Task,
    parent: Option<ParentTask>,
}

impl TaskData {
    pub fn new((action, description): (String, Option<String>)) -> Option<Self> {
        let description = match description {
            Some(description) => description,
            None => return None,
        };

        let action = match action.split_once(":") {
            Some((target, action)) if target == "indices" => action,
            _ => return None,
        };

        let kind = match action.splitn(3, "/").collect::<Vec<&str>>()[1..] {
            [operation, kind] if operation == "write" => kind,
            _ => return None,
        };

        let (is_shard, is_primary) = match kind.splitn(3, "[").collect::<Vec<&str>>() {
            parts if parts.len() == 3 => (parts[1] == "s]", parts[2] == "p]"),
            parts if parts.len() == 2 => (parts[1] == "s]", false),
            parts if parts.len() == 1 => (false, false),
            _ => return None,
        };

        let (docs, description) = match description
            .trim_start_matches("requests[")
            .split_once("], ")
        {
            Some((docs, description)) => (docs.parse::<u64>().ok(), description),
            None => (None, description.as_str()),
        };

        match is_shard {
            true => {
                let (index, number): (String, Option<u32>) =
                    match description.trim_start_matches("index[").split_once("][") {
                        Some((index, number)) => (
                            index.into(),
                            number.trim_end_matches("]").parse::<u32>().ok(),
                        ),
                        None => (String::default(), None),
                    };

                Some(TaskData {
                    index: Some(TaskIndex {
                        name: Some(vec![index]),
                        count: Some(1),
                        bulk: None,
                    }),
                    shard: Some(TaskShard {
                        number: number,
                        primary: Some(is_primary),
                        bulk: Some(BulkDocs { docs }),
                    }),
                })
            }
            false => {
                let indices = description
                    .trim_start_matches("indices[")
                    .trim_end_matches("]")
                    .split(",")
                    .map(|s| s.trim().into())
                    .collect::<Vec<String>>();

                Some(TaskData {
                    index: Some(TaskIndex {
                        count: Some(indices.len() as u32),
                        name: Some(indices),
                        bulk: Some(BulkDocs { docs }),
                    }),
                    shard: None,
                })
            }
        }
    }
}
