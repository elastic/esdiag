use super::metadata::Metadata;
use json_patch::merge;
use serde_json::{json, Value};

pub async fn enrich(metadata: &Metadata, data: Value) -> Vec<Value> {
    let mut tasks = Vec::<Value>::new();
    let data = match data["nodes"].as_object() {
        Some(data) => data,
        None => return tasks,
    };
    let data_stream = json!({
        "data_stream": {
            "dataset": "task",
            "namespace": "esdiag",
            "type": "metrics",
        }
    });
    for (node_id, node) in data {
        for (id, task) in node["tasks"].as_object().unwrap() {
            let task_patch = json!({
                "@timestamp": metadata.diagnostic.collection_date,
                "node": metadata.lookup.node.by_id(node_id.as_str()),
                "cluster": metadata.cluster,
                "diagnostic": metadata.diagnostic,
                "task": { "id": id, },
            });
            let mut task = task.clone();
            merge(&mut task, &task_patch);
            merge(&mut task, &data_stream);
            tasks.push(task);
        }
    }
    log::debug!("task docs: {}", tasks.len());
    tasks
}
