// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! `metrics-kubernetes.volume-esdiag`: one document per PersistentVolumeClaim,
//! joined with its bound PersistentVolume and StorageClass.

use super::{
    DocumentExporter, EckMetadata, ProcessorSummary,
    bundle::{EckLookups, InNamespace, VOLUME_STREAM},
    fields::{EckRef, KubernetesRef, condition_map},
    quantity,
    resource::send,
    resources::{Condition, PersistentVolumeClaim, RawQuantities},
};
use crate::exporter::Exporter;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub struct Volumes(pub Vec<InNamespace<PersistentVolumeClaim>>);

#[derive(Serialize)]
struct VolumeDoc {
    kubernetes: KubernetesRef,
    #[serde(skip_serializing_if = "EckRef::is_empty")]
    eck: EckRef,
    claim: ClaimFields,
    #[serde(skip_serializing_if = "Option::is_none")]
    requested_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capacity_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resize_pending: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    conditions: Vec<Condition>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    condition: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    volume: Option<VolumeFields>,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage_class: Option<StorageClassFields>,
}

#[derive(Serialize)]
struct ClaimFields {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    phase: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    access_modes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    volume_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    volume_name: Option<String>,
}

#[derive(Serialize)]
struct VolumeFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    reclaim_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    csi_driver: Option<String>,
}

#[derive(Serialize)]
struct StorageClassFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    provisioner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allow_volume_expansion: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    volume_binding_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reclaim_policy: Option<String>,
}

fn storage(quantities: Option<&RawQuantities>) -> Option<f64> {
    quantities?.get("storage").and_then(|value| quantity::parse(value))
}

fn volume_doc(claim: InNamespace<PersistentVolumeClaim>, lookups: &EckLookups) -> VolumeDoc {
    let InNamespace { namespace, item } = claim;
    let name = item.metadata.name().to_string();
    let pod = lookups.claim_pods.get(&(namespace.clone(), name.clone()));
    let requested_bytes = storage(item.spec.resources.requests.as_ref());
    let capacity_bytes = storage(item.status.capacity.as_ref());
    let volume = item
        .spec
        .volume_name
        .as_ref()
        .and_then(|volume| lookups.volumes.get(volume));
    let storage_class = item
        .spec
        .storage_class_name
        .as_ref()
        .and_then(|class| lookups.storage_classes.get(class));

    VolumeDoc {
        kubernetes: KubernetesRef::namespace(&namespace).pod(pod.map(String::as_str)),
        eck: EckRef::from_labels(&item.metadata.labels),
        resize_pending: requested_bytes
            .zip(capacity_bytes)
            .map(|(requested, capacity)| requested > capacity),
        requested_bytes,
        capacity_bytes,
        condition: condition_map(&item.status.conditions),
        conditions: item.status.conditions,
        volume: volume.map(|volume| VolumeFields {
            reclaim_policy: volume.reclaim_policy.clone(),
            csi_driver: volume.csi_driver.clone(),
        }),
        storage_class: storage_class.map(|class| StorageClassFields {
            provisioner: class.provisioner.clone(),
            allow_volume_expansion: class.allow_volume_expansion,
            volume_binding_mode: class.volume_binding_mode.clone(),
            reclaim_policy: class.reclaim_policy.clone(),
        }),
        claim: ClaimFields {
            name,
            phase: item.status.phase,
            access_modes: item.spec.access_modes,
            storage_class: item.spec.storage_class_name,
            volume_mode: item.spec.volume_mode,
            volume_name: item.spec.volume_name,
        },
    }
}

impl DocumentExporter<EckLookups, EckMetadata> for Volumes {
    async fn documents_export(
        self,
        exporter: &Exporter,
        lookups: &EckLookups,
        metadata: &EckMetadata,
    ) -> ProcessorSummary {
        let docs: Vec<Value> = self
            .0
            .into_iter()
            .map(|claim| metadata.document(VOLUME_STREAM, volume_doc(claim, lookups)))
            .collect();
        send(exporter, VOLUME_STREAM, docs).await
    }
}
