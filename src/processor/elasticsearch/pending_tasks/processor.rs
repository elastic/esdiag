// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    super::{DataProcessor, ElasticsearchMetadata, Lookups, Metadata},
    PendingTask, PendingTasks,
};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;

impl DataProcessor<Lookups, ElasticsearchMetadata> for PendingTasks {
    fn generate_docs(
        self,
        _lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> (String, Vec<Value>) {
        log::debug!("processing pending tasks");
        let data_stream = "metrics-task.pending-esdiag".to_string();
        let metadata = metadata.for_data_stream(&data_stream).as_meta_doc();

        let mut pending_tasks: Vec<PendingTask> = self.tasks.into_par_iter().collect();

        let pending_tasks: Vec<Value> = pending_tasks
            .par_drain(..)
            .filter_map(|task| {
                serde_json::to_value(PendingTaskDoc {
                    task,
                    metadata: metadata.clone(),
                })
                .ok()
            })
            .collect();

        log::debug!("pending task docs: {}", pending_tasks.len());
        (data_stream, pending_tasks)
    }
}

#[derive(Serialize)]
struct PendingTaskDoc {
    task: PendingTask,
    #[serde(flatten)]
    metadata: Value,
}
