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
use serde_json::{Map, Value};

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
    status: Option<Value>,
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
        status: item.status.as_ref().and_then(allowed_status),
        generation_lag: item
            .metadata
            .generation
            .zip(observed)
            .map(|(generation, observed)| generation - observed),
    }
}

/// Operator-written status fields kept on resource documents. Values are
/// copied only when they are scalars, so nothing nested in an unexpected
/// field reaches the index.
const STATUS_FIELDS: [&str; 6] = [
    "health",
    "phase",
    "version",
    "availableNodes",
    "expectedNodes",
    "observedGeneration",
];
const CONDITION_FIELDS: [&str; 5] = ["type", "status", "reason", "message", "lastTransitionTime"];
/// ECK serializes in-progress operations as `downscale`, `upgrade`, and `upscale`.
const OPERATIONS: [&str; 3] = ["downscale", "upgrade", "upscale"];
const OPERATION_FIELDS: [&str; 2] = ["lastUpdatedTime", "stalled"];
const OPERATION_NODE_FIELDS: [&str; 7] = [
    "name",
    "status",
    "state",
    "shutdownStatus",
    "predicate",
    "message",
    "explanation",
];

fn scalars(object: &Map<String, Value>, keys: &[&str]) -> Map<String, Value> {
    keys.iter()
        .filter_map(|key| {
            let value = object.get(*key)?;
            (value.is_string() || value.is_number() || value.is_boolean()).then(|| (key.to_string(), value.clone()))
        })
        .collect()
}

fn scalar_list(value: Option<&Value>, keys: &[&str]) -> Option<Value> {
    let items: Vec<Value> = value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .map(|item| Value::Object(scalars(item, keys)))
        .collect();
    (!items.is_empty()).then_some(Value::Array(items))
}

fn allowed_status(status: &Value) -> Option<Value> {
    let status = status.as_object()?;
    let mut allowed = scalars(status, &STATUS_FIELDS);
    if let Some(conditions) = scalar_list(status.get("conditions"), &CONDITION_FIELDS) {
        allowed.insert("conditions".to_string(), conditions);
    }
    if let Some(operations) = status.get("inProgressOperations").and_then(Value::as_object) {
        let operations: Map<String, Value> = OPERATIONS
            .iter()
            .filter_map(|key| {
                let operation = operations.get(*key)?.as_object()?;
                let mut fields = scalars(operation, &OPERATION_FIELDS);
                if let Some(nodes) = scalar_list(operation.get("nodes"), &OPERATION_NODE_FIELDS) {
                    fields.insert("nodes".to_string(), nodes);
                }
                Some((key.to_string(), Value::Object(fields)))
            })
            .collect();
        if !operations.is_empty() {
            allowed.insert("inProgressOperations".to_string(), Value::Object(operations));
        }
    }
    // `associationStatus`, `kibanaAssociationStatus`, and the per-reference
    // `elasticsearchAssociationsStatus` map.
    for (key, value) in status {
        let key_lower = key.to_ascii_lowercase();
        if key_lower.ends_with("associationstatus") && value.is_string() {
            allowed.insert(key.clone(), value.clone());
        } else if key_lower.ends_with("associationsstatus")
            && let Some(statuses) = value.as_object()
        {
            let statuses: Map<String, Value> = statuses
                .iter()
                .filter(|(_, status)| status.is_string())
                .map(|(name, status)| (name.clone(), status.clone()))
                .collect();
            if !statuses.is_empty() {
                allowed.insert(key.clone(), Value::Object(statuses));
            }
        }
    }
    (!allowed.is_empty()).then_some(Value::Object(allowed))
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
    fn status_keeps_only_allowlisted_scalars() {
        let status = allowed_status(&json!({
            "health": "green",
            "availableNodes": 3,
            "annotations": {"secret": "annotation-sentinel"},
            "env": [{"name": "PASSWORD", "value": "env-sentinel"}],
            "conditions": [{"type": "ReconciliationComplete", "status": "True", "extra": {"value": "nested-sentinel"}}],
            "inProgressOperations": {
                "upgrade": {"lastUpdatedTime": "t", "nodes": [{"name": "es-0", "status": "PENDING", "spec": {}}]},
                "upgradeOperation": {"value": "operation-sentinel"}
            },
            "kibanaAssociationStatus": "Established",
            "elasticsearchAssociationsStatus": {"demo/quickstart": "Established", "demo/other": {"value": "map-sentinel"}},
            "message": {"nested": "message-sentinel"}
        }))
        .unwrap();
        assert_eq!(
            status,
            json!({
                "health": "green",
                "availableNodes": 3,
                "conditions": [{"type": "ReconciliationComplete", "status": "True"}],
                "inProgressOperations": {
                    "upgrade": {"lastUpdatedTime": "t", "nodes": [{"name": "es-0", "status": "PENDING"}]}
                },
                "kibanaAssociationStatus": "Established",
                "elasticsearchAssociationsStatus": {"demo/quickstart": "Established"}
            })
        );
        assert_eq!(allowed_status(&json!({"unknown": 1})), None);
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
