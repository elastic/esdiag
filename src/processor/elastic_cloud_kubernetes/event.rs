// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! `logs-kubernetes.event-esdiag`: one document per Kubernetes event.

use super::{
    DocumentExporter, EckMetadata, ProcessorSummary,
    bundle::{CUSTOM_RESOURCES, EVENT_STREAM, EckLookups, InNamespace},
    fields::{EckRef, KubernetesRef},
    resource::send,
    resources::{Event, ObjectReference},
};
use crate::exporter::Exporter;
use serde::Serialize;
use serde_json::{Value, json};

pub struct Events(pub Vec<InNamespace<Event>>);

#[derive(Serialize)]
struct EventDoc {
    #[serde(rename = "@timestamp")]
    timestamp: Value,
    kubernetes: KubernetesRef,
    #[serde(skip_serializing_if = "EckRef::is_empty")]
    eck: EckRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<SourceFields>,
    involved_object: ObjectReference,
}

#[derive(Serialize)]
struct SourceFields {
    component: String,
}

fn event_doc(event: InNamespace<Event>, collection_date: u64) -> EventDoc {
    let InNamespace { namespace, item } = event;
    let timestamp = item
        .last_timestamp
        .clone()
        .or_else(|| item.event_time.clone())
        .or_else(|| item.first_timestamp.clone())
        .map_or_else(|| json!(collection_date), Value::from);
    let involved = &item.involved_object;
    let pod = (involved.kind.as_deref() == Some("Pod"))
        .then_some(involved.name.as_deref())
        .flatten();
    let namespace = involved.namespace.clone().unwrap_or(namespace);
    let eck = involved
        .kind
        .as_deref()
        .zip(involved.name.as_deref())
        .filter(|(kind, _)| CUSTOM_RESOURCES.iter().any(|(_, eck_kind)| eck_kind == kind))
        .map(|(kind, name)| EckRef::resource(kind, name))
        .unwrap_or_default();

    EventDoc {
        timestamp,
        kubernetes: KubernetesRef::namespace(&namespace).pod(pod),
        eck,
        r#type: item.r#type,
        reason: item.reason,
        message: item.message,
        count: item.count,
        first_timestamp: item.first_timestamp,
        last_timestamp: item.last_timestamp,
        source: [
            item.reporting_component,
            item.reporting_controller,
            item.source.component,
        ]
        .into_iter()
        .flatten()
        .find(|component| !component.is_empty())
        .map(|component| SourceFields { component }),
        involved_object: item.involved_object,
    }
}

impl DocumentExporter<EckLookups, EckMetadata> for Events {
    async fn documents_export(self, exporter: &Exporter, _: &EckLookups, metadata: &EckMetadata) -> ProcessorSummary {
        let collection_date = metadata.collection_date();
        let docs: Vec<Value> = self
            .0
            .into_iter()
            .map(|event| metadata.document(EVENT_STREAM, event_doc(event, collection_date)))
            .collect();
        send(exporter, EVENT_STREAM, docs).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(value: Value) -> InNamespace<Event> {
        InNamespace {
            namespace: "demo".to_string(),
            item: serde_json::from_value(value).unwrap(),
        }
    }

    #[test]
    fn timestamp_falls_back_through_event_times() {
        let doc = event_doc(
            event(json!({"lastTimestamp": "b", "eventTime": "c", "firstTimestamp": "a"})),
            7,
        );
        assert_eq!(doc.timestamp, "b");
        let doc = event_doc(event(json!({"eventTime": "c", "firstTimestamp": "a"})), 7);
        assert_eq!(doc.timestamp, "c");
        let doc = event_doc(event(json!({"firstTimestamp": "a"})), 7);
        assert_eq!(doc.timestamp, "a");
        let doc = event_doc(event(json!({})), 7);
        assert_eq!(doc.timestamp, 7);
    }

    #[test]
    fn pod_events_carry_the_pod_join_key() {
        let doc = event_doc(
            event(json!({
                "type": "Warning",
                "reason": "FailedScheduling",
                "involvedObject": {"kind": "Pod", "name": "quickstart-es-data-1", "namespace": "demo", "fieldPath": "spec"}
            })),
            7,
        );
        let doc = serde_json::to_value(doc).unwrap();
        assert_eq!(doc["kubernetes"]["pod"]["name"], "quickstart-es-data-1");
        assert_eq!(doc["involved_object"]["field_path"], "spec");
        assert_eq!(doc["reason"], "FailedScheduling");
    }

    #[test]
    fn reporter_falls_back_from_component_to_controller_to_source() {
        let reporter =
            |value: Value| serde_json::to_value(event_doc(event(value), 7)).unwrap()["source"]["component"].clone();
        assert_eq!(
            reporter(json!({"reportingComponent": "a", "reportingController": "b", "source": {"component": "c"}})),
            "a"
        );
        assert_eq!(
            reporter(json!({"reportingComponent": "", "reportingController": "kibana-controller"})),
            "kibana-controller"
        );
        assert_eq!(reporter(json!({"source": {"component": "kubelet"}})), "kubelet");
        assert_eq!(reporter(json!({})), Value::Null);
    }
}
