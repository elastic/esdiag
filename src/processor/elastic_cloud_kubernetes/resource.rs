// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! `settings-eck.resource-esdiag` and `settings-eck.nodeset-esdiag`.

use super::{
    DocumentExporter, EckMetadata, ProcessorSummary,
    bundle::{EckLookups, InNamespace, NODESET_STREAM, RESOURCE_STREAM},
    fields::{EckRef, KubernetesRef, Named, Resources as ContainerResources},
    labels::{self, Heap},
    quantity,
    resources::{EckResource, NodeSet, StatefulSetStatus},
};
use crate::exporter::Exporter;
use serde::Serialize;
use serde_json::Value;

const ELASTICSEARCH_CONTAINER: &str = "elasticsearch";

pub struct Resources<'a>(pub &'a [InNamespace<EckResource>]);

#[derive(Serialize)]
struct ResourceDoc<'a> {
    eck: EckRef,
    kubernetes: KubernetesRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    spec: Option<SpecFields<'a>>,
    metadata: MetadataFields,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_lag: Option<i64>,
}

#[derive(Serialize)]
struct SpecFields<'a> {
    version: &'a str,
}

#[derive(Serialize)]
struct MetadataFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    generation: Option<i64>,
}

fn resource_doc(resource: &InNamespace<EckResource>) -> ResourceDoc<'_> {
    let item = &resource.item;
    let observed = item
        .status
        .as_ref()
        .and_then(|status| status.get("observedGeneration"))
        .and_then(Value::as_i64);
    ResourceDoc {
        eck: EckRef::resource(item.kind.as_deref().unwrap_or_default(), item.metadata.name()),
        kubernetes: KubernetesRef::namespace(&resource.namespace),
        spec: item.spec.version.as_deref().map(|version| SpecFields { version }),
        metadata: MetadataFields {
            generation: item.metadata.generation,
        },
        status: item.status.as_ref(),
        generation_lag: item
            .metadata
            .generation
            .zip(observed)
            .map(|(generation, observed)| generation - observed),
    }
}

impl DocumentExporter<EckLookups, EckMetadata> for Resources<'_> {
    async fn documents_export(self, exporter: &Exporter, _: &EckLookups, metadata: &EckMetadata) -> ProcessorSummary {
        let docs: Vec<Value> = self
            .0
            .iter()
            .map(|resource| metadata.document(RESOURCE_STREAM, resource_doc(resource)))
            .collect();
        send(exporter, RESOURCE_STREAM, docs).await
    }
}

pub struct NodeSets<'a>(pub &'a [InNamespace<EckResource>]);

#[derive(Serialize)]
struct NodeSetDoc {
    eck: EckRef,
    kubernetes: KubernetesRef,
    elasticsearch: Named,
    nodeset: NodeSetFields,
    #[serde(skip_serializing_if = "ContainerResources::is_empty")]
    resources: ContainerResources,
    #[serde(skip_serializing_if = "Option::is_none")]
    jvm: Option<JvmFields>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    volume_claims: Vec<VolumeClaim>,
    #[serde(skip_serializing_if = "Option::is_none")]
    statefulset: Option<StatefulSetFields>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rollout_in_progress: Option<bool>,
}

#[derive(Serialize)]
struct NodeSetFields {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    roles: Option<Vec<String>>,
}

#[derive(Serialize)]
struct JvmFields {
    heap: HeapFields,
}

#[derive(Serialize)]
struct HeapFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    min_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_bytes: Option<f64>,
}

impl From<Heap> for JvmFields {
    fn from(heap: Heap) -> Self {
        Self {
            heap: HeapFields {
                min_bytes: heap.min_bytes,
                max_bytes: heap.max_bytes,
            },
        }
    }
}

#[derive(Serialize)]
struct VolumeClaim {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage_class: Option<String>,
}

#[derive(Serialize)]
struct StatefulSetFields {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    replicas: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ready_replicas: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_replicas: Option<u32>,
}

/// `node.roles` from a nodeSet config written either as a dotted key or as
/// nested objects. `None` means unset, which Elasticsearch treats as all roles.
fn node_roles(config: Option<&Value>) -> Option<Vec<String>> {
    let config = config?;
    let roles = config
        .get("node.roles")
        .or_else(|| config.get("node").and_then(|node| node.get("roles")))?;
    Some(
        roles
            .as_array()?
            .iter()
            .filter_map(|role| role.as_str().map(str::to_string))
            .collect(),
    )
}

fn nodeset_doc(namespace: &str, cluster: &str, nodeset: &NodeSet, lookups: &EckLookups) -> NodeSetDoc {
    let name = nodeset.name.clone().unwrap_or_default();
    let container = nodeset
        .pod_template
        .spec
        .containers
        .iter()
        .find(|container| container.name.as_deref() == Some(ELASTICSEARCH_CONTAINER));
    let statefulset_name = format!("{cluster}-es-{name}");
    let statefulset: Option<&StatefulSetStatus> = lookups
        .statefulsets
        .get(&(namespace.to_string(), statefulset_name.clone()));

    NodeSetDoc {
        eck: EckRef::resource("Elasticsearch", cluster),
        kubernetes: KubernetesRef::namespace(namespace),
        elasticsearch: Named::from(cluster),
        nodeset: NodeSetFields {
            name,
            count: nodeset.count,
            roles: node_roles(nodeset.config.as_ref()),
        },
        resources: container
            .map(|container| ContainerResources::from(&container.resources))
            .unwrap_or_default(),
        jvm: container
            .and_then(|container| container.env.as_deref())
            .and_then(labels::heap)
            .map(JvmFields::from),
        volume_claims: nodeset
            .volume_claim_templates
            .iter()
            .map(|template| VolumeClaim {
                name: template.metadata.name.clone(),
                storage_bytes: template
                    .spec
                    .resources
                    .requests
                    .as_ref()
                    .and_then(|requests| requests.get("storage"))
                    .and_then(|storage| quantity::parse(storage)),
                storage_class: template.spec.storage_class_name.clone(),
            })
            .collect(),
        rollout_in_progress: statefulset
            .and_then(|status| Some(status.current_revision.as_ref()? != status.update_revision.as_ref()?)),
        statefulset: statefulset.map(|status| StatefulSetFields {
            name: statefulset_name,
            replicas: status.replicas,
            ready_replicas: status.ready_replicas,
            updated_replicas: status.updated_replicas,
        }),
    }
}

impl DocumentExporter<EckLookups, EckMetadata> for NodeSets<'_> {
    async fn documents_export(
        self,
        exporter: &Exporter,
        lookups: &EckLookups,
        metadata: &EckMetadata,
    ) -> ProcessorSummary {
        let docs: Vec<Value> = self
            .0
            .iter()
            .filter(|resource| resource.item.kind.as_deref() == Some("Elasticsearch"))
            .flat_map(|resource| {
                resource.item.spec.node_sets.iter().map(|nodeset| {
                    metadata.document(
                        NODESET_STREAM,
                        nodeset_doc(&resource.namespace, resource.item.metadata.name(), nodeset, lookups),
                    )
                })
            })
            .collect();
        send(exporter, NODESET_STREAM, docs).await
    }
}

/// Sends one stream's documents as a single batch.
pub async fn send(exporter: &Exporter, stream: &str, docs: Vec<Value>) -> ProcessorSummary {
    let mut summary = ProcessorSummary::new(stream.to_string());
    match exporter.send(stream.to_string(), docs).await {
        Ok(batch) => summary.add_batch(batch),
        Err(err) => tracing::error!("Failed to send {}: {}", stream, err),
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn generation_lag_is_generation_minus_observed() {
        let resource = InNamespace {
            namespace: "demo".to_string(),
            item: serde_json::from_value::<EckResource>(json!({
                "kind": "Elasticsearch",
                "metadata": {"name": "quickstart", "generation": 5},
                "spec": {"version": "9.3.3"},
                "status": {"observedGeneration": 4, "health": "yellow"}
            }))
            .unwrap(),
        };
        let doc = serde_json::to_value(resource_doc(&resource)).unwrap();
        assert_eq!(doc["generation_lag"], 1);
        assert_eq!(doc["status"]["health"], "yellow");
        assert_eq!(doc["eck"]["resource"]["kind"], "Elasticsearch");
        assert_eq!(doc["kubernetes"]["namespace"], "demo");
    }

    #[test]
    fn node_roles_accept_dotted_and_nested_config() {
        assert_eq!(
            node_roles(Some(&json!({"node.roles": ["master"]}))),
            Some(vec!["master".to_string()])
        );
        assert_eq!(node_roles(Some(&json!({"node": {"roles": []}}))), Some(Vec::new()));
        assert_eq!(node_roles(Some(&json!({"xpack.ml.enabled": true}))), None);
        assert_eq!(node_roles(None), None);
    }
}
