// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! `metrics-kubernetes.pod-esdiag` and `metrics-kubernetes.container-esdiag`.

use super::{
    DocumentExporter, EckMetadata, ProcessorSummary,
    bundle::{CONTAINER_STREAM, EckLookups, InNamespace, POD_STREAM},
    fields::{EckRef, KubernetesRef, Resources, condition_map},
    labels,
    resource::send,
    resources::{Condition, ContainerState, ContainerStatus, Pod},
};
use crate::exporter::Exporter;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

fn kubernetes_ref(pod: &InNamespace<Pod>) -> KubernetesRef {
    KubernetesRef::namespace(&pod.namespace)
        .pod(Some(pod.item.metadata.name()))
        .node(pod.item.spec.node_name.as_deref())
        .labels(&pod.item.metadata.labels)
}

pub struct Pods<'a>(pub &'a [InNamespace<Pod>]);

#[derive(Serialize)]
struct PodDoc<'a> {
    kubernetes: KubernetesRef,
    #[serde(skip_serializing_if = "EckRef::is_empty")]
    eck: EckRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    elasticsearch: Option<ElasticsearchRoles>,
    status: PodStatusFields<'a>,
    #[serde(skip_serializing_if = "<[Condition]>::is_empty")]
    conditions: &'a [Condition],
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    condition: BTreeMap<String, String>,
    restart_count: u32,
    not_ready_containers: usize,
}

#[derive(Serialize)]
struct ElasticsearchRoles {
    roles: Vec<String>,
}

#[derive(Serialize)]
struct PodStatusFields<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    phase: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    qos_class: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_time: Option<&'a str>,
}

fn pod_doc(pod: &InNamespace<Pod>) -> PodDoc<'_> {
    let item = &pod.item;
    let status = &item.status;
    let ready = status
        .container_statuses
        .iter()
        .filter(|container| container.ready == Some(true))
        .count();
    let roles = labels::elasticsearch_roles(&item.metadata.labels);
    PodDoc {
        kubernetes: kubernetes_ref(pod),
        eck: EckRef::from_labels(&item.metadata.labels),
        elasticsearch: (!roles.is_empty()).then_some(ElasticsearchRoles { roles }),
        status: PodStatusFields {
            phase: status.phase.as_deref(),
            qos_class: status.qos_class.as_deref(),
            start_time: status.start_time.as_deref(),
        },
        conditions: &status.conditions,
        condition: condition_map(&status.conditions),
        restart_count: status
            .init_container_statuses
            .iter()
            .chain(&status.container_statuses)
            .filter_map(|container| container.restart_count)
            .sum(),
        not_ready_containers: item
            .spec
            .containers
            .len()
            .max(status.container_statuses.len())
            .saturating_sub(ready),
    }
}

impl DocumentExporter<EckLookups, EckMetadata> for Pods<'_> {
    async fn documents_export(self, exporter: &Exporter, _: &EckLookups, metadata: &EckMetadata) -> ProcessorSummary {
        let docs: Vec<Value> = self
            .0
            .iter()
            .map(|pod| metadata.document(POD_STREAM, pod_doc(pod)))
            .collect();
        send(exporter, POD_STREAM, docs).await
    }
}

pub struct Containers<'a>(pub &'a [InNamespace<Pod>]);

#[derive(Serialize)]
struct ContainerDoc<'a> {
    kubernetes: KubernetesRef,
    #[serde(skip_serializing_if = "EckRef::is_empty")]
    eck: EckRef,
    container: ContainerFields<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<StateFields<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_terminated: Option<TerminatedFields<'a>>,
    #[serde(skip_serializing_if = "Resources::is_empty")]
    resources: Resources,
}

#[derive(Serialize)]
struct ContainerFields<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    init: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_tag: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ready: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    restart_count: Option<u32>,
}

#[derive(Serialize)]
struct StateFields<'a> {
    current: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
}

#[derive(Serialize)]
struct TerminatedFields<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    finished_at: Option<&'a str>,
}

fn current_state(state: &ContainerState) -> Option<StateFields<'_>> {
    if let Some(running) = &state.running {
        return Some(StateFields {
            current: "running",
            reason: None,
            started_at: running.started_at.as_deref(),
            exit_code: None,
        });
    }
    if let Some(waiting) = &state.waiting {
        return Some(StateFields {
            current: "waiting",
            reason: waiting.reason.as_deref(),
            started_at: None,
            exit_code: None,
        });
    }
    state.terminated.as_ref().map(|terminated| StateFields {
        current: "terminated",
        reason: terminated.reason.as_deref(),
        started_at: terminated.started_at.as_deref(),
        exit_code: terminated.exit_code,
    })
}

fn container_doc<'a>(pod: &'a InNamespace<Pod>, status: &'a ContainerStatus, init: bool) -> ContainerDoc<'a> {
    let spec = if init {
        &pod.item.spec.init_containers
    } else {
        &pod.item.spec.containers
    };
    let resources = spec
        .iter()
        .find(|container| container.name.is_some() && container.name == status.name)
        .map(|container| Resources::from(&container.resources))
        .unwrap_or_default();
    ContainerDoc {
        kubernetes: kubernetes_ref(pod),
        eck: EckRef::from_labels(&pod.item.metadata.labels),
        container: ContainerFields {
            name: status.name.as_deref(),
            init,
            image_tag: status.image.as_deref().and_then(labels::image_tag),
            ready: status.ready,
            started: status.started,
            restart_count: status.restart_count,
        },
        state: current_state(&status.state),
        last_terminated: status
            .last_state
            .terminated
            .as_ref()
            .map(|terminated| TerminatedFields {
                reason: terminated.reason.as_deref(),
                exit_code: terminated.exit_code,
                finished_at: terminated.finished_at.as_deref(),
            }),
        resources,
    }
}

impl DocumentExporter<EckLookups, EckMetadata> for Containers<'_> {
    async fn documents_export(self, exporter: &Exporter, _: &EckLookups, metadata: &EckMetadata) -> ProcessorSummary {
        let docs: Vec<Value> = self
            .0
            .iter()
            .flat_map(|pod| {
                let init = pod
                    .item
                    .status
                    .init_container_statuses
                    .iter()
                    .map(move |status| (pod, status, true));
                let app = pod
                    .item
                    .status
                    .container_statuses
                    .iter()
                    .map(move |status| (pod, status, false));
                init.chain(app)
            })
            .map(|(pod, status, init)| metadata.document(CONTAINER_STREAM, container_doc(pod, status, init)))
            .collect();
        send(exporter, CONTAINER_STREAM, docs).await
    }
}
