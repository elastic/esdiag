// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! The subset of `kubectl get -o json` output ESDiag reads from an ECK bundle.
//!
//! These structs are the field allowlist: serde ignores every field they do
//! not name, so environment values, annotations, `managedFields`, and
//! ConfigMap or Secret payloads cannot reach an indexed document. Every field
//! is optional because the bundle format drifts across Kubernetes and
//! `eck-diagnostics` versions.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub type RawQuantities = BTreeMap<String, String>;

/// A `kubectl` `List`. `secrets.json` spells the key `Items`.
#[derive(Deserialize)]
pub struct KubeList<T> {
    #[serde(default = "Vec::new", alias = "Items")]
    pub items: Vec<T>,
}

impl<T> Default for KubeList<T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ObjectMeta {
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub generation: Option<i64>,
    pub labels: BTreeMap<String, String>,
}

impl ObjectMeta {
    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or_default()
    }

    pub fn label(&self, key: &str) -> Option<&str> {
        self.labels.get(key).map(String::as_str)
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Condition {
    pub r#type: Option<String>,
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(rename(deserialize = "lastTransitionTime"), skip_serializing_if = "Option::is_none")]
    pub last_transition_time: Option<String>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct ResourceRequirements {
    pub limits: Option<RawQuantities>,
    pub requests: Option<RawQuantities>,
}

// Nodes

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct Node {
    pub metadata: ObjectMeta,
    pub spec: NodeSpec,
    pub status: NodeStatus,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct NodeSpec {
    pub taints: Vec<Taint>,
    pub unschedulable: Option<bool>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Taint {
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub effect: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NodeStatus {
    pub capacity: Option<RawQuantities>,
    pub allocatable: Option<RawQuantities>,
    pub conditions: Vec<Condition>,
    pub node_info: NodeInfo,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct NodeInfo {
    pub architecture: Option<String>,
    #[serde(rename(deserialize = "containerRuntimeVersion"))]
    pub container_runtime_version: Option<String>,
    #[serde(rename(deserialize = "kernelVersion"))]
    pub kernel_version: Option<String>,
    #[serde(rename(deserialize = "kubeProxyVersion"))]
    pub kube_proxy_version: Option<String>,
    #[serde(rename(deserialize = "kubeletVersion"))]
    pub kubelet_version: Option<String>,
    #[serde(rename(deserialize = "operatingSystem"))]
    pub operating_system: Option<String>,
    #[serde(rename(deserialize = "osImage"))]
    pub os_image: Option<String>,
}

// Pods

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct Pod {
    pub metadata: ObjectMeta,
    pub spec: PodSpec,
    pub status: PodStatus,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PodSpec {
    pub node_name: Option<String>,
    pub init_containers: Vec<Container>,
    pub containers: Vec<Container>,
    pub volumes: Vec<Volume>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct Container {
    pub name: Option<String>,
    pub image: Option<String>,
    pub resources: ResourceRequirements,
    /// `Always` on an init container marks a native sidecar.
    #[serde(rename = "restartPolicy")]
    pub restart_policy: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Volume {
    pub persistent_volume_claim: Option<ClaimSource>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ClaimSource {
    pub claim_name: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PodStatus {
    pub phase: Option<String>,
    pub qos_class: Option<String>,
    pub start_time: Option<String>,
    pub conditions: Vec<Condition>,
    pub init_container_statuses: Vec<ContainerStatus>,
    pub container_statuses: Vec<ContainerStatus>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ContainerStatus {
    pub name: Option<String>,
    pub image: Option<String>,
    pub ready: Option<bool>,
    pub started: Option<bool>,
    pub restart_count: Option<u32>,
    pub state: ContainerState,
    pub last_state: ContainerState,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct ContainerState {
    pub running: Option<RunningState>,
    pub waiting: Option<WaitingState>,
    pub terminated: Option<TerminatedState>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunningState {
    pub started_at: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct WaitingState {
    pub reason: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TerminatedState {
    pub exit_code: Option<i32>,
    pub reason: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

// StatefulSets

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct StatefulSet {
    pub metadata: ObjectMeta,
    pub spec: StatefulSetSpec,
    pub status: StatefulSetStatus,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct StatefulSetSpec {
    pub template: PodTemplate<Container>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StatefulSetStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ready_replicas: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_replicas: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_replicas: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_revision: Option<String>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct PodTemplate<C> {
    pub spec: PodTemplateSpec<C>,
}

#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct PodTemplateSpec<C> {
    pub containers: Vec<C>,
}

impl<C> Default for PodTemplateSpec<C> {
    fn default() -> Self {
        Self { containers: Vec::new() }
    }
}

// Volumes

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct PersistentVolumeClaim {
    pub metadata: ObjectMeta,
    pub spec: ClaimSpec,
    pub status: ClaimStatus,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ClaimSpec {
    pub access_modes: Vec<String>,
    pub resources: ResourceRequirements,
    pub storage_class_name: Option<String>,
    pub volume_mode: Option<String>,
    pub volume_name: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct ClaimStatus {
    pub phase: Option<String>,
    pub capacity: Option<RawQuantities>,
    pub conditions: Vec<Condition>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct PersistentVolume {
    pub metadata: ObjectMeta,
    pub spec: VolumeSpec,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VolumeSpec {
    pub csi: Option<CsiSource>,
    pub persistent_volume_reclaim_policy: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct CsiSource {
    pub driver: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StorageClass {
    pub metadata: ObjectMeta,
    pub provisioner: Option<String>,
    pub allow_volume_expansion: Option<bool>,
    pub reclaim_policy: Option<String>,
    pub volume_binding_mode: Option<String>,
}

// Events

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Event {
    pub metadata: ObjectMeta,
    pub r#type: Option<String>,
    pub reason: Option<String>,
    pub message: Option<String>,
    pub count: Option<u32>,
    pub first_timestamp: Option<String>,
    pub last_timestamp: Option<String>,
    pub event_time: Option<String>,
    pub reporting_component: Option<String>,
    pub reporting_controller: Option<String>,
    pub source: EventSource,
    pub involved_object: ObjectReference,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct EventSource {
    pub component: Option<String>,
    pub host: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ObjectReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(rename(serialize = "field_path"), skip_serializing_if = "Option::is_none")]
    pub field_path: Option<String>,
}

// ECK custom resources

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct EckResource {
    pub kind: Option<String>,
    pub metadata: ObjectMeta,
    pub spec: EckSpec,
    /// Operator-written status, reduced to an allowlist before indexing.
    pub status: Option<Value>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EckSpec {
    pub version: Option<String>,
    pub node_sets: Vec<NodeSet>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NodeSet {
    pub name: Option<String>,
    pub count: Option<u32>,
    /// Elasticsearch settings; only `node.roles` is read from it.
    pub config: Option<Value>,
    pub pod_template: PodTemplate<TemplateContainer>,
    pub volume_claim_templates: Vec<VolumeClaimTemplate>,
}

/// A nodeSet pod template container. `env` is read solely to extract the JVM
/// heap from `ES_JAVA_OPTS`; no other variable is retained.
#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct TemplateContainer {
    pub name: Option<String>,
    pub resources: ResourceRequirements,
    #[serde(deserialize_with = "deserialize_java_opts")]
    pub env: Option<String>,
}

fn deserialize_java_opts<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct EnvVar {
        name: Option<String>,
        value: Option<String>,
    }
    let vars: Option<Vec<EnvVar>> = Option::deserialize(deserializer)?;
    Ok(vars
        .into_iter()
        .flatten()
        .find(|var| var.name.as_deref() == Some("ES_JAVA_OPTS"))
        .and_then(|var| var.value))
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct VolumeClaimTemplate {
    pub metadata: ObjectMeta,
    pub spec: ClaimSpec,
}

// ConfigMaps

/// A ConfigMap reduced to the two ECK-owned payloads ESDiag reads. Every
/// other ConfigMap's data is discarded during deserialization.
#[derive(Default, Deserialize)]
#[serde(from = "RawConfigMap")]
pub struct ConfigMap {
    pub operator_config: Option<String>,
    pub licensing: Option<BTreeMap<String, String>>,
}

pub const OPERATOR_CONFIG_MAP: &str = "elastic-operator";
pub const LICENSING_CONFIG_MAP: &str = "elastic-licensing";
const LICENSING_KEYS: [&str; 4] = [
    "eck_license_level",
    "enterprise_resource_units",
    "timestamp",
    "total_managed_memory_bytes",
];

#[derive(Default, Deserialize)]
#[serde(default)]
struct RawConfigMap {
    metadata: ObjectMeta,
    data: BTreeMap<String, String>,
}

impl From<RawConfigMap> for ConfigMap {
    fn from(raw: RawConfigMap) -> Self {
        let name = raw.metadata.name();
        let mut data = raw.data;
        let operator_config = (name == OPERATOR_CONFIG_MAP).then(|| data.remove("eck.yaml")).flatten();
        let licensing = (name == LICENSING_CONFIG_MAP).then(|| {
            data.into_iter()
                .filter(|(key, _)| LICENSING_KEYS.contains(&key.as_str()))
                .collect()
        });
        Self {
            operator_config,
            licensing,
        }
    }
}

// Root metadata files

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct BundleVersion {
    pub diagnostics_version: DiagnosticsVersion,
    pub server_version: ServerVersion,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct DiagnosticsVersion {
    pub version: Option<String>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ServerVersion {
    #[serde(rename(deserialize = "gitVersion"))]
    pub git_version: Option<String>,
    pub major: Option<String>,
    pub minor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn user_config_map_data_is_discarded() {
        let config_map: ConfigMap = serde_json::from_value(json!({
            "metadata": {"name": "pipelines"},
            "data": {"eck.yaml": "user-owned", "enterprise_resource_units": "9"}
        }))
        .unwrap();
        assert!(config_map.operator_config.is_none());
        assert!(config_map.licensing.is_none());
    }

    #[test]
    fn eck_config_maps_keep_only_allowlisted_keys() {
        let operator: ConfigMap = serde_json::from_value(json!({
            "metadata": {"name": OPERATOR_CONFIG_MAP},
            "data": {"eck.yaml": "namespaces: [demo]", "other": "dropped"}
        }))
        .unwrap();
        assert_eq!(operator.operator_config.as_deref(), Some("namespaces: [demo]"));

        let licensing: ConfigMap = serde_json::from_value(json!({
            "metadata": {"name": LICENSING_CONFIG_MAP},
            "data": {"eck_license_level": "basic", "unexpected": "dropped"}
        }))
        .unwrap();
        let licensing = licensing.licensing.unwrap();
        assert_eq!(licensing.len(), 1);
        assert_eq!(licensing["eck_license_level"], "basic");
    }

    #[test]
    fn template_env_keeps_only_java_opts() {
        let container: TemplateContainer = serde_json::from_value(json!({
            "name": "elasticsearch",
            "env": [
                {"name": "SECRET", "value": "plaintext"},
                {"name": "ES_JAVA_OPTS", "value": "-Xms1g -Xmx1g"}
            ]
        }))
        .unwrap();
        assert_eq!(container.env.as_deref(), Some("-Xms1g -Xmx1g"));
    }

    #[test]
    fn secrets_list_uses_capitalized_items_key() {
        let list: KubeList<ObjectMeta> = serde_json::from_value(json!({"Items": [{}, {}]})).unwrap();
        assert_eq!(list.items.len(), 2);
    }
}
