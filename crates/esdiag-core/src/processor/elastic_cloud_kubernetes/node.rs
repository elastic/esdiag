// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! `metrics-kubernetes.node-esdiag`: one document per Kubernetes node.

use super::{
    DocumentExporter, EckMetadata, ProcessorSummary,
    bundle::{EckLookups, NODE_STREAM, Scheduled},
    fields::{KubernetesRef, condition_map},
    labels,
    quantity::Quantities,
    resource::send,
    resources::{Condition, Node, NodeInfo, Taint},
};
use crate::exporter::Exporter;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub struct Nodes(pub Vec<Node>);

#[derive(Serialize)]
struct NodeDoc {
    kubernetes: KubernetesRef,
    #[serde(skip_serializing_if = "Quantities::is_empty")]
    capacity: Quantities,
    #[serde(skip_serializing_if = "Quantities::is_empty")]
    allocatable: Quantities,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    conditions: Vec<Condition>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    condition: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    taints: Vec<Taint>,
    unschedulable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    zone: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    roles: Vec<String>,
    node_info: NodeInfo,
    scheduled: Scheduled,
}

fn node_doc(node: Node, lookups: &EckLookups) -> NodeDoc {
    let name = node.metadata.name().to_string();
    NodeDoc {
        scheduled: lookups.scheduled.get(&name).cloned().unwrap_or_default(),
        kubernetes: KubernetesRef::default().node(Some(&name)),
        capacity: Quantities::from_raw(node.status.capacity.as_ref()),
        allocatable: Quantities::from_raw(node.status.allocatable.as_ref()),
        condition: condition_map(&node.status.conditions),
        conditions: node.status.conditions,
        taints: node.spec.taints,
        unschedulable: node.spec.unschedulable.unwrap_or(false),
        zone: node.metadata.label(labels::ZONE_LABEL).map(str::to_string),
        roles: labels::node_roles(&node.metadata.labels),
        node_info: node.status.node_info,
    }
}

impl DocumentExporter<EckLookups, EckMetadata> for Nodes {
    async fn documents_export(
        self,
        exporter: &Exporter,
        lookups: &EckLookups,
        metadata: &EckMetadata,
    ) -> ProcessorSummary {
        let docs: Vec<Value> = self
            .0
            .into_iter()
            .map(|node| metadata.document(NODE_STREAM, node_doc(node, lookups)))
            .collect();
        send(exporter, NODE_STREAM, docs).await
    }
}
