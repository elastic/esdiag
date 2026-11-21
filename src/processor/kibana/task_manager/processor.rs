// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, KibanaMetadata, Lookups, Metadata, document, send_documents};
use super::TaskManagerHealth;
use crate::{exporter::Exporter, processor::ProcessorSummary};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// Task Manager health sections keyed by task type, and the `task_type` field
/// each section becomes. Task type names contain dots, so the maps cannot be
/// indexed as objects without expanding every name into nested fields.
const TASK_TYPE_SECTIONS: [(&str, &[&str]); 4] = [
    ("workload", &["stats", "workload", "value", "task_types"]),
    ("drift", &["stats", "runtime", "value", "drift_by_type"]),
    ("duration", &["stats", "runtime", "value", "execution", "duration"]),
    (
        "result_frequency",
        &[
            "stats",
            "runtime",
            "value",
            "execution",
            "result_frequency_percent_as_number",
        ],
    ),
];

impl DocumentExporter<Lookups, KibanaMetadata> for TaskManagerHealth {
    async fn documents_export(
        mut self,
        exporter: &Exporter,
        _: &Lookups,
        metadata: &KibanaMetadata,
    ) -> ProcessorSummary {
        let task_types = take_task_types(&mut self.0);

        let data_stream = "metrics-kibana.task_manager-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let doc = document(&metadata_doc, self.0);
        let mut summary = send_documents(exporter, &data_stream, vec![doc]).await;

        if !task_types.is_empty() {
            let data_stream = "metrics-kibana.task_manager.task_type-esdiag".to_string();
            let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
            let docs: Vec<Value> = task_types
                .into_iter()
                .map(|(name, mut sections)| {
                    sections.insert("name".to_string(), Value::String(name));
                    document(&metadata_doc, json!({ "task_type": sections }))
                })
                .collect();
            summary.add_child(Ok(send_documents(exporter, &data_stream, docs).await));
        }

        summary
    }
}

/// Removes the per-task-type maps from a Task Manager health response and
/// regroups their entries by task type name.
fn take_task_types(health: &mut Value) -> BTreeMap<String, Map<String, Value>> {
    let mut task_types: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
    for (section, path) in TASK_TYPE_SECTIONS {
        let Some(Value::Object(entries)) = take_path(health, path) else {
            continue;
        };
        for (name, value) in entries {
            task_types.entry(name).or_default().insert(section.to_string(), value);
        }
    }
    task_types
}

fn take_path(value: &mut Value, path: &[&str]) -> Option<Value> {
    let (last, parents) = path.split_last()?;
    let parent = parents.iter().try_fold(value, |node, key| node.get_mut(*key))?;
    parent.as_object_mut()?.remove(*last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn health() -> Value {
        json!({
            "status": "OK",
            "stats": {
                "configuration": {"status": "OK", "value": {"poll_interval": 500}},
                "workload": {"status": "OK", "value": {
                    "count": 3,
                    "task_types": {
                        "alerting:slo.rules.burnRate": {"count": 2, "status": {"idle": 2}},
                        "Fleet-Metrics-Task": {"count": 1, "status": {"idle": 1}}
                    }
                }},
                "runtime": {"status": "OK", "value": {
                    "drift": {"p50": 10, "p99": 400.5},
                    "drift_by_type": {
                        "alerting:slo.rules.burnRate": {"p50": 12, "p99": 30.5},
                        "endpoint:user-artifact-packager": {"p50": 493.5, "p99": 513}
                    },
                    "execution": {
                        "duration": {"alerting:slo.rules.burnRate": {"p50": 9, "p99": 26}},
                        "result_frequency_percent_as_number": {
                            "alerting:slo.rules.burnRate": {"Success": 100, "RetryScheduled": 0, "Failed": 0, "status": "OK"}
                        },
                        "persistence": {"recurring": 100}
                    }
                }},
                "capacity_estimation": {"status": "OK", "value": {"observed": {"observed_kibana_instances": 1}}}
            }
        })
    }

    #[test]
    fn task_type_entries_merge_across_sections() {
        let mut health = health();
        let task_types = take_task_types(&mut health);

        let burn_rate = &task_types["alerting:slo.rules.burnRate"];
        assert_eq!(burn_rate["workload"]["count"], 2);
        assert_eq!(burn_rate["drift"]["p99"], 30.5);
        assert_eq!(burn_rate["duration"]["p50"], 9);
        assert_eq!(burn_rate["result_frequency"]["Success"], 100);
        assert_eq!(task_types.len(), 3);
    }

    #[test]
    fn task_type_in_some_sections_omits_absent_sections() {
        let mut health = health();
        let task_types = take_task_types(&mut health);

        let fleet = &task_types["Fleet-Metrics-Task"];
        assert_eq!(fleet.keys().collect::<Vec<_>>(), vec!["workload"]);
        let packager = &task_types["endpoint:user-artifact-packager"];
        assert_eq!(packager.keys().collect::<Vec<_>>(), vec!["drift"]);
    }

    #[test]
    fn summary_keeps_aggregate_stats_without_task_type_maps() {
        let mut health = health();
        take_task_types(&mut health);

        let workload = &health["stats"]["workload"]["value"];
        let runtime = &health["stats"]["runtime"]["value"];
        assert!(workload.get("task_types").is_none());
        assert!(runtime.get("drift_by_type").is_none());
        assert!(runtime["execution"].get("duration").is_none());
        assert!(runtime["execution"].get("result_frequency_percent_as_number").is_none());
        assert_eq!(health["status"], "OK");
        assert_eq!(health["stats"]["configuration"]["value"]["poll_interval"], 500);
        assert_eq!(workload["count"], 3);
        assert_eq!(runtime["drift"]["p99"], 400.5);
        assert_eq!(runtime["execution"]["persistence"]["recurring"], 100);
        assert_eq!(
            health["stats"]["capacity_estimation"]["value"]["observed"]["observed_kibana_instances"],
            1
        );
    }

    #[test]
    fn response_without_task_types_yields_none() {
        let mut health = json!({"status": "OK", "stats": {"configuration": {"value": {}}}});
        assert!(take_task_types(&mut health).is_empty());
        assert_eq!(health["status"], "OK");
    }
}
