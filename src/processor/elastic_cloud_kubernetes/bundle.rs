// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Reading an ECK bundle: which files are sources, how they are discovered,
//! and the lookups built from them before any document is exported.

use super::{
    DocumentExporter, EckMetadata, ProcessorSummary,
    event::Events,
    labels,
    logs::{self, ToolLog},
    node::Nodes,
    operator::Operator,
    operator_log::OperatorLogs,
    pod::{Containers, Pods},
    resource::{NodeSets, Resources},
    resources::{
        BundleVersion, ConfigMap, EckResource, Event, KubeList, Node, PersistentVolume, PersistentVolumeClaim, Pod,
        StatefulSet, StatefulSetStatus, StorageClass,
    },
    volume::Volumes,
};
use crate::{
    exporter::Exporter,
    receiver::{MissingSource, Receiver},
};
use eyre::{Result, eyre};
use futures::TryStreamExt;
use serde::de::DeserializeOwned;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use tokio::sync::mpsc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scope {
    /// A file at the bundle root.
    Cluster,
    /// A file in each namespace directory.
    Namespaced,
}

/// A bundle file ESDiag reads. `stream` is `None` for files that only
/// enrich documents of other sources.
#[derive(Clone, Copy, Debug)]
pub struct EckSource {
    pub key: &'static str,
    pub file: &'static str,
    pub scope: Scope,
    pub stream: Option<&'static str>,
}

impl EckSource {
    /// The bundle path of this source in `namespace` (ignored for cluster
    /// sources).
    pub fn path(&self, namespace: &str) -> String {
        match self.scope {
            Scope::Cluster => self.file.to_string(),
            Scope::Namespaced => format!("{namespace}/{}", self.file),
        }
    }

    fn stream(&self) -> &'static str {
        self.stream.unwrap_or(self.key)
    }
}

const fn source(key: &'static str, file: &'static str, scope: Scope, stream: Option<&'static str>) -> EckSource {
    EckSource {
        key,
        file,
        scope,
        stream,
    }
}

pub const OPERATOR_STREAM: &str = "settings-eck.operator-esdiag";
pub const RESOURCE_STREAM: &str = "settings-eck.resource-esdiag";
pub const NODESET_STREAM: &str = "settings-eck.nodeset-esdiag";
pub const OPERATOR_LOG_STREAM: &str = "logs-eck.operator-esdiag";
pub const NODE_STREAM: &str = "metrics-kubernetes.node-esdiag";
pub const POD_STREAM: &str = "metrics-kubernetes.pod-esdiag";
pub const CONTAINER_STREAM: &str = "metrics-kubernetes.container-esdiag";
pub const VOLUME_STREAM: &str = "metrics-kubernetes.volume-esdiag";
pub const EVENT_STREAM: &str = "logs-kubernetes.event-esdiag";

pub const TOOL_LOG: EckSource = source(
    "eck_tool_log",
    "eck-diagnostics.log",
    Scope::Cluster,
    Some(OPERATOR_STREAM),
);
pub const VERSION: EckSource = source("eck_version", "version.json", Scope::Cluster, None);
pub const COLLECTION_ERRORS: EckSource = source(
    "eck_collection_errors",
    "eck-diagnostic-errors.txt",
    Scope::Cluster,
    None,
);
pub const CONFIG_MAPS: EckSource = source("kubernetes_configmaps", "configmaps.json", Scope::Namespaced, None);
pub const NODES: EckSource = source("kubernetes_nodes", "nodes.json", Scope::Cluster, Some(NODE_STREAM));
pub const STORAGE_CLASSES: EckSource = source("kubernetes_storageclasses", "storageclasses.json", Scope::Cluster, None);
pub const PODS: EckSource = source("kubernetes_pods", "pods.json", Scope::Namespaced, Some(POD_STREAM));
pub const CONTAINERS: EckSource = source(
    "kubernetes_containers",
    "pods.json",
    Scope::Namespaced,
    Some(CONTAINER_STREAM),
);
pub const STATEFULSETS: EckSource = source("kubernetes_statefulsets", "statefulsets.json", Scope::Namespaced, None);
pub const CLAIMS: EckSource = source(
    "kubernetes_persistentvolumeclaims",
    "persistentvolumeclaims.json",
    Scope::Namespaced,
    Some(VOLUME_STREAM),
);
pub const VOLUMES: EckSource = source(
    "kubernetes_persistentvolumes",
    "persistentvolumes.json",
    Scope::Namespaced,
    None,
);
pub const EVENTS: EckSource = source(
    "kubernetes_events",
    "events.json",
    Scope::Namespaced,
    Some(EVENT_STREAM),
);
pub const OPERATOR_LOGS: EckSource = source(
    "eck_operator_logs",
    "pod/{pod}/logs.txt",
    Scope::Namespaced,
    Some(OPERATOR_LOG_STREAM),
);
pub const NODE_SETS: EckSource = source(
    "eck_nodesets",
    "elasticsearch.json",
    Scope::Namespaced,
    Some(NODESET_STREAM),
);

/// ECK custom resource lists and the kind each holds.
pub const CUSTOM_RESOURCES: [(EckSource, &str); 8] = [
    (
        source(
            "eck_elasticsearch",
            "elasticsearch.json",
            Scope::Namespaced,
            Some(RESOURCE_STREAM),
        ),
        "Elasticsearch",
    ),
    (
        source("eck_kibana", "kibana.json", Scope::Namespaced, Some(RESOURCE_STREAM)),
        "Kibana",
    ),
    (
        source("eck_agent", "agent.json", Scope::Namespaced, Some(RESOURCE_STREAM)),
        "Agent",
    ),
    (
        source(
            "eck_logstash",
            "logstash.json",
            Scope::Namespaced,
            Some(RESOURCE_STREAM),
        ),
        "Logstash",
    ),
    (
        source("eck_beat", "beat.json", Scope::Namespaced, Some(RESOURCE_STREAM)),
        "Beat",
    ),
    (
        source(
            "eck_apmserver",
            "apmserver.json",
            Scope::Namespaced,
            Some(RESOURCE_STREAM),
        ),
        "ApmServer",
    ),
    (
        source(
            "eck_enterprisesearch",
            "enterprisesearch.json",
            Scope::Namespaced,
            Some(RESOURCE_STREAM),
        ),
        "EnterpriseSearch",
    ),
    (
        source(
            "eck_elasticmapsserver",
            "elasticmapsserver.json",
            Scope::Namespaced,
            Some(RESOURCE_STREAM),
        ),
        "ElasticMapsServer",
    ),
];

/// Every file the processor reads; the bundle inventory in `diagnostic.md`.
#[cfg(test)]
pub fn sources() -> Vec<EckSource> {
    let mut sources = vec![
        TOOL_LOG,
        VERSION,
        COLLECTION_ERRORS,
        CONFIG_MAPS,
        NODES,
        STORAGE_CLASSES,
        PODS,
        CONTAINERS,
        STATEFULSETS,
        CLAIMS,
        VOLUMES,
        EVENTS,
        OPERATOR_LOGS,
        NODE_SETS,
    ];
    sources.extend(CUSTOM_RESOURCES.iter().map(|(source, _)| *source));
    sources
}

/// Whether a source's file was found and parsed, tracked per output stream.
#[derive(Default)]
pub struct SourceStatus {
    found: bool,
    errors: Vec<String>,
}

impl SourceStatus {
    fn record<T>(&mut self, path: &str, result: &Result<Option<T>>) {
        match result {
            Ok(Some(_)) => self.found = true,
            Ok(None) => {}
            Err(err) => self.errors.push(format!("{path}: {err:#}")),
        }
    }

    fn merge(&mut self, other: &SourceStatus) {
        self.found |= other.found;
        self.errors.extend(other.errors.iter().cloned());
    }

    /// The final summary for `stream`: parsed when any file was found,
    /// missing when none was and nothing failed.
    pub fn summarize(&self, stream: &str, summary: ProcessorSummary) -> ProcessorSummary {
        let summary = if self.found {
            summary.was_parsed()
        } else if self.errors.is_empty() {
            ProcessorSummary::missing(stream.to_string())
        } else {
            summary
        };
        self.errors
            .iter()
            .fold(summary, |summary, error| summary.with_error(error.clone()))
    }
}

pub fn is_missing_source_error(err: &eyre::Report) -> bool {
    err.chain().any(|cause| {
        cause.is::<MissingSource>()
            || cause
                .downcast_ref::<std::io::Error>()
                .is_some_and(|err| err.kind() == std::io::ErrorKind::NotFound)
    })
}

async fn read_json<T: DeserializeOwned>(receiver: &Receiver, path: &str) -> Result<Option<T>> {
    match receiver.read_bundle_json::<T>(path).await {
        Ok(value) => Ok(Some(value)),
        Err(err) if is_missing_source_error(&err) => Ok(None),
        Err(err) => Err(err),
    }
}

async fn read_text(receiver: &Receiver, path: &str) -> Result<Option<String>> {
    match receiver.read_bundle_lines(path).await {
        Ok(lines) => {
            let lines: Vec<String> = lines.try_collect().await?;
            Ok(Some(lines.join("\n")))
        }
        Err(err) if is_missing_source_error(&err) => Ok(None),
        Err(err) => Err(err),
    }
}

/// Reads a `kubectl` list, recording its outcome in `status`. Missing,
/// empty, and unparseable files all yield no items.
async fn read_list<T: DeserializeOwned>(receiver: &Receiver, path: &str, status: &mut SourceStatus) -> Vec<T> {
    let result = read_json::<KubeList<T>>(receiver, path).await;
    status.record(path, &result);
    result.ok().flatten().unwrap_or_default().items
}

fn tag<T>(namespace: &str, items: impl IntoIterator<Item = T>) -> impl Iterator<Item = InNamespace<T>> {
    items.into_iter().map(|item| InNamespace {
        namespace: namespace.to_string(),
        item,
    })
}

/// Namespaced items tagged with the namespace directory they were read from.
pub struct InNamespace<T> {
    pub namespace: String,
    pub item: T,
}

/// The first state: the bundle's namespace directories are known.
pub struct Discovered {
    namespaces: Vec<String>,
}

/// The second state: every source is loaded and lookups are built.
pub struct Indexed {
    lookups: EckLookups,
    operator: Operator,
    nodes: Vec<Node>,
    pods: Vec<InNamespace<Pod>>,
    claims: Vec<InNamespace<PersistentVolumeClaim>>,
    events: Vec<InNamespace<Event>>,
    resources: Vec<InNamespace<EckResource>>,
    status: HashMap<&'static str, SourceStatus>,
}

pub struct EckBundle<State> {
    receiver: Arc<Receiver>,
    state: State,
}

impl EckBundle<Discovered> {
    /// Lists root directories as candidate namespaces. Directories with no
    /// recognized files simply yield no documents.
    pub async fn discover(receiver: Arc<Receiver>) -> Result<Self> {
        let namespaces = receiver.list_bundle_dirs("").await?;
        Ok(Self {
            receiver,
            state: Discovered { namespaces },
        })
    }

    pub fn namespaces(&self) -> &[String] {
        &self.state.namespaces
    }

    pub async fn index(self) -> Result<EckBundle<Indexed>> {
        let receiver = self.receiver.as_ref();
        let mut status: HashMap<&'static str, SourceStatus> = HashMap::new();

        let nodes: Vec<Node> = read_list(receiver, &NODES.path(""), status.entry(NODES.key).or_default()).await;
        let storage_classes: Vec<StorageClass> = read_list(
            receiver,
            &STORAGE_CLASSES.path(""),
            status.entry(STORAGE_CLASSES.key).or_default(),
        )
        .await;

        let mut pods = Vec::new();
        let mut claims = Vec::new();
        let mut events = Vec::new();
        let mut resources = Vec::new();
        let mut statefulsets = Vec::new();
        let mut volumes = HashMap::new();
        let mut config_maps = Vec::new();

        for namespace in &self.state.namespaces {
            let path = |source: &EckSource| source.path(namespace);

            let mut pod_status = SourceStatus::default();
            let items = read_list(receiver, &path(&PODS), &mut pod_status).await;
            for source in [PODS, CONTAINERS] {
                status.entry(source.key).or_default().merge(&pod_status);
            }
            pods.extend(tag(namespace, items));
            let items = read_list(receiver, &path(&CLAIMS), status.entry(CLAIMS.key).or_default()).await;
            claims.extend(tag(namespace, items));
            let items = read_list(receiver, &path(&EVENTS), status.entry(EVENTS.key).or_default()).await;
            events.extend(tag(namespace, items));
            let items = read_list(
                receiver,
                &path(&STATEFULSETS),
                status.entry(STATEFULSETS.key).or_default(),
            )
            .await;
            statefulsets.extend(tag(namespace, items));
            let items = read_list(
                receiver,
                &path(&CONFIG_MAPS),
                status.entry(CONFIG_MAPS.key).or_default(),
            )
            .await;
            config_maps.extend(tag(namespace, items));
            // `persistentvolumes.json` is cluster-wide, repeated per namespace.
            let items: Vec<PersistentVolume> =
                read_list(receiver, &path(&VOLUMES), status.entry(VOLUMES.key).or_default()).await;
            for volume in items {
                volumes.entry(volume.metadata.name().to_string()).or_insert(volume);
            }

            for (source, kind) in &CUSTOM_RESOURCES {
                let mut file_status = SourceStatus::default();
                let items: Vec<EckResource> = read_list(receiver, &path(source), &mut file_status).await;
                status.entry(RESOURCE_STREAM).or_default().merge(&file_status);
                if source.file == NODE_SETS.file {
                    status.entry(NODE_SETS.key).or_default().merge(&file_status);
                }
                resources.extend(tag(
                    namespace,
                    items.into_iter().map(|mut resource| {
                        resource.kind.get_or_insert_with(|| kind.to_string());
                        resource
                    }),
                ));
            }
        }

        let lookups = EckLookups::new(&storage_classes, volumes, &pods, &claims, &statefulsets);
        let operator = self
            .operator(&lookups, &statefulsets, &pods, config_maps, &mut status)
            .await;

        Ok(EckBundle {
            receiver: self.receiver,
            state: Indexed {
                lookups,
                operator,
                nodes,
                pods,
                claims,
                events,
                resources,
                status,
            },
        })
    }

    async fn operator(
        &self,
        lookups: &EckLookups,
        statefulsets: &[InNamespace<StatefulSet>],
        pods: &[InNamespace<Pod>],
        config_maps: Vec<InNamespace<ConfigMap>>,
        status: &mut HashMap<&'static str, SourceStatus>,
    ) -> Operator {
        let receiver = self.receiver.as_ref();
        let tool_log = read_text(receiver, &TOOL_LOG.path("")).await;
        status.entry(TOOL_LOG.key).or_default().record(TOOL_LOG.file, &tool_log);
        let tool_log = tool_log
            .ok()
            .flatten()
            .map(|text| logs::parse_tool_log(&text))
            .unwrap_or_default();

        let version = read_json::<BundleVersion>(receiver, &VERSION.path("")).await;
        status.entry(VERSION.key).or_default().record(VERSION.file, &version);
        let version = version.ok().flatten().unwrap_or_default();

        let errors = read_text(receiver, &COLLECTION_ERRORS.path("")).await;
        status
            .entry(COLLECTION_ERRORS.key)
            .or_default()
            .record(COLLECTION_ERRORS.file, &errors);
        let errors = errors.ok().flatten().and_then(|text| logs::collection_errors(&text));

        let config_maps = operator_config_maps(config_maps, &tool_log.operator_namespaces, statefulsets);
        let operator_images = operator_images(statefulsets, pods, lookups);
        let eck_version = logs::eck_version(&tool_log, operator_images.iter().map(String::as_str));

        Operator::new(
            eck_version,
            version,
            ToolLog {
                eck_version: None,
                ..tool_log
            },
            errors,
            config_maps,
        )
    }
}

/// ConfigMaps from the operator's own namespaces: those the tool log names, or
/// failing that, those holding an operator-labeled StatefulSet. A workload
/// namespace may own an unrelated ConfigMap named `elastic-operator`.
fn operator_config_maps(
    config_maps: Vec<InNamespace<ConfigMap>>,
    logged_namespaces: &[String],
    statefulsets: &[InNamespace<StatefulSet>],
) -> Vec<ConfigMap> {
    let labeled_namespaces: Vec<&str> = statefulsets
        .iter()
        .filter(|statefulset| labels::is_operator(&statefulset.item.metadata.labels))
        .map(|statefulset| statefulset.namespace.as_str())
        .collect();
    let is_operator_namespace = |namespace: &str| {
        if logged_namespaces.is_empty() {
            labeled_namespaces.contains(&namespace)
        } else {
            logged_namespaces.iter().any(|logged| logged == namespace)
        }
    };
    config_maps
        .into_iter()
        .filter(|config_map| is_operator_namespace(&config_map.namespace))
        .map(|config_map| config_map.item)
        .collect()
}

/// Operator container images: the operator StatefulSet template first, then
/// the running operator pods.
fn operator_images(
    statefulsets: &[InNamespace<StatefulSet>],
    pods: &[InNamespace<Pod>],
    lookups: &EckLookups,
) -> Vec<String> {
    let template_images = statefulsets
        .iter()
        .filter(|statefulset| {
            labels::is_operator(&statefulset.item.metadata.labels)
                || statefulset.item.metadata.name() == "elastic-operator"
        })
        .flat_map(|statefulset| statefulset.item.spec.template.spec.containers.iter())
        .filter_map(|container| container.image.clone());
    let pod_images = pods
        .iter()
        .filter(|pod| lookups.is_operator_pod(&pod.namespace, pod.item.metadata.name()))
        .flat_map(|pod| pod.item.spec.containers.iter())
        .filter_map(|container| container.image.clone());
    template_images.chain(pod_images).collect()
}

impl EckBundle<Indexed> {
    /// Exports every stream, sending one summary per stream.
    pub async fn export(
        self,
        exporter: &Exporter,
        metadata: &EckMetadata,
        summary_tx: &mpsc::Sender<ProcessorSummary>,
    ) -> Result<()> {
        let Indexed {
            lookups,
            operator,
            nodes,
            pods,
            claims,
            events,
            resources,
            status,
        } = self.state;
        let status_of = |key: &str| status.get(key).unwrap_or(&EMPTY_STATUS);
        let mut operator_status = SourceStatus::default();
        for source in [TOOL_LOG, VERSION, COLLECTION_ERRORS, CONFIG_MAPS] {
            operator_status.merge(status_of(source.key));
        }

        let summaries = [
            operator_status.summarize(
                TOOL_LOG.stream(),
                operator.documents_export(exporter, &lookups, metadata).await,
            ),
            status_of(RESOURCE_STREAM).summarize(
                RESOURCE_STREAM,
                Resources(&resources)
                    .documents_export(exporter, &lookups, metadata)
                    .await,
            ),
            status_of(NODE_SETS.key).summarize(
                NODE_SETS.stream(),
                NodeSets(&resources)
                    .documents_export(exporter, &lookups, metadata)
                    .await,
            ),
            status_of(NODES.key).summarize(
                NODES.stream(),
                Nodes(nodes).documents_export(exporter, &lookups, metadata).await,
            ),
            status_of(PODS.key).summarize(
                PODS.stream(),
                Pods(&pods).documents_export(exporter, &lookups, metadata).await,
            ),
            status_of(CONTAINERS.key).summarize(
                CONTAINERS.stream(),
                Containers(&pods).documents_export(exporter, &lookups, metadata).await,
            ),
            status_of(CLAIMS.key).summarize(
                CLAIMS.stream(),
                Volumes(claims).documents_export(exporter, &lookups, metadata).await,
            ),
            status_of(EVENTS.key).summarize(
                EVENTS.stream(),
                Events(events).documents_export(exporter, &lookups, metadata).await,
            ),
        ];
        for summary in summaries {
            send_summary(summary_tx, summary).await?;
        }

        let logs = OperatorLogs {
            receiver: self.receiver,
            pods: lookups
                .operator_pods
                .iter()
                .map(|(namespace, pod)| {
                    let path = OPERATOR_LOGS.path(namespace).replace("{pod}", pod);
                    (namespace.clone(), pod.clone(), path)
                })
                .collect(),
        };
        let summary = logs.documents_export(exporter, &lookups, metadata).await;
        send_summary(summary_tx, summary).await
    }
}

static EMPTY_STATUS: SourceStatus = SourceStatus {
    found: false,
    errors: Vec::new(),
};

async fn send_summary(summary_tx: &mpsc::Sender<ProcessorSummary>, summary: ProcessorSummary) -> Result<()> {
    summary_tx.send(summary).await.map_err(|err| {
        tracing::error!("Failed to send summary: {}", err);
        eyre!(err.to_string())
    })
}

/// Scheduled requests and limits summed over the collected pods on a node.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct Scheduled {
    pub pods: u32,
    pub requests: BTreeMap<String, f64>,
    pub limits: BTreeMap<String, f64>,
}

#[derive(Clone)]
pub struct VolumeInfo {
    pub reclaim_policy: Option<String>,
    pub csi_driver: Option<String>,
}

#[derive(Clone)]
pub struct StorageClassInfo {
    pub provisioner: Option<String>,
    pub allow_volume_expansion: Option<bool>,
    pub volume_binding_mode: Option<String>,
    pub reclaim_policy: Option<String>,
}

/// Derived maps built once in `index`; every enrichment reads from them.
pub struct EckLookups {
    pub storage_classes: HashMap<String, StorageClassInfo>,
    pub volumes: HashMap<String, VolumeInfo>,
    /// (namespace, claim) → mounting pod.
    pub claim_pods: HashMap<(String, String), String>,
    /// node → scheduled sums.
    pub scheduled: HashMap<String, Scheduled>,
    /// (namespace, name) → StatefulSet status.
    pub statefulsets: HashMap<(String, String), StatefulSetStatus>,
    /// (namespace, pod) of every operator pod.
    pub operator_pods: Vec<(String, String)>,
}

const SCHEDULED_RESOURCES: [&str; 2] = ["cpu", "memory"];

impl EckLookups {
    fn new(
        storage_classes: &[StorageClass],
        volumes: HashMap<String, PersistentVolume>,
        pods: &[InNamespace<Pod>],
        claims: &[InNamespace<PersistentVolumeClaim>],
        statefulsets: &[InNamespace<StatefulSet>],
    ) -> Self {
        let storage_classes = storage_classes
            .iter()
            .map(|class| {
                (
                    class.metadata.name().to_string(),
                    StorageClassInfo {
                        provisioner: class.provisioner.clone(),
                        allow_volume_expansion: class.allow_volume_expansion,
                        volume_binding_mode: class.volume_binding_mode.clone(),
                        reclaim_policy: class.reclaim_policy.clone(),
                    },
                )
            })
            .collect();

        let bound_volumes: Vec<&str> = claims
            .iter()
            .filter_map(|claim| claim.item.spec.volume_name.as_deref())
            .collect();
        let volumes = volumes
            .into_iter()
            .filter(|(name, _)| bound_volumes.contains(&name.as_str()))
            .map(|(name, volume)| {
                (
                    name,
                    VolumeInfo {
                        reclaim_policy: volume.spec.persistent_volume_reclaim_policy,
                        csi_driver: volume.spec.csi.and_then(|csi| csi.driver),
                    },
                )
            })
            .collect();

        let mut claim_pods = HashMap::new();
        let mut scheduled: HashMap<String, Scheduled> = HashMap::new();
        let mut operator_pods = Vec::new();
        for pod in pods {
            let name = pod.item.metadata.name().to_string();
            for claim in pod
                .item
                .spec
                .volumes
                .iter()
                .filter_map(|volume| volume.persistent_volume_claim.as_ref()?.claim_name.clone())
            {
                claim_pods.insert((pod.namespace.clone(), claim), name.clone());
            }
            if labels::is_operator(&pod.item.metadata.labels) {
                operator_pods.push((pod.namespace.clone(), name.clone()));
            }
            let terminal = matches!(pod.item.status.phase.as_deref(), Some("Succeeded" | "Failed"));
            if let (Some(node), false) = (pod.item.spec.node_name.as_ref(), terminal) {
                let sums = scheduled.entry(node.clone()).or_default();
                sums.pods += 1;
                for resource in SCHEDULED_RESOURCES {
                    let (requests, limits) = pod_effective(&pod.item, resource);
                    add(&mut sums.requests, resource, requests);
                    add(&mut sums.limits, resource, limits);
                }
            }
        }

        let statefulsets = statefulsets
            .iter()
            .map(|statefulset| {
                (
                    (
                        statefulset.namespace.clone(),
                        statefulset.item.metadata.name().to_string(),
                    ),
                    statefulset.item.status.clone(),
                )
            })
            .collect();

        Self {
            storage_classes,
            volumes,
            claim_pods,
            scheduled,
            statefulsets,
            operator_pods,
        }
    }

    pub fn is_operator_pod(&self, namespace: &str, pod: &str) -> bool {
        self.operator_pods
            .iter()
            .any(|(pod_namespace, name)| pod_namespace == namespace && name == pod)
    }
}

fn add(sums: &mut BTreeMap<String, f64>, resource: &str, value: Option<f64>) {
    if let Some(value) = value {
        *sums.entry(resource.to_string()).or_default() += value;
    }
}

/// A pod's effective request and limit for `resource`, as the scheduler
/// computes it: the larger of the app container sum and the largest init
/// container.
fn pod_effective(pod: &Pod, resource: &str) -> (Option<f64>, Option<f64>) {
    let quantity = |raw: Option<&BTreeMap<String, String>>| {
        raw.and_then(|raw| raw.get(resource))
            .and_then(|value| super::quantity::parse(value))
    };
    let effective = |pick: fn(&super::resources::ResourceRequirements) -> Option<&BTreeMap<String, String>>| {
        let app: Vec<f64> = pod
            .spec
            .containers
            .iter()
            .filter_map(|container| quantity(pick(&container.resources)))
            .collect();
        let init = pod
            .spec
            .init_containers
            .iter()
            .filter_map(|container| quantity(pick(&container.resources)))
            .reduce(f64::max);
        let app = (!app.is_empty()).then(|| app.iter().sum::<f64>());
        match (app, init) {
            (Some(app), Some(init)) => Some(app.max(init)),
            (app, init) => app.or(init),
        }
    };
    (
        effective(|resources| resources.requests.as_ref()),
        effective(|resources| resources.limits.as_ref()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn operator_config_comes_only_from_operator_namespaces() {
        let config_map = |namespace: &str, yaml: &str| InNamespace {
            namespace: namespace.to_string(),
            item: serde_json::from_value::<ConfigMap>(json!({
                "metadata": {"name": "elastic-operator"},
                "data": {"eck.yaml": yaml}
            }))
            .unwrap(),
        };
        let config_maps = || {
            vec![
                config_map("demo", "decoy: true"),
                config_map("elastic-system", "real: true"),
            ]
        };
        let configs =
            |maps: Vec<ConfigMap>| -> Vec<String> { maps.into_iter().filter_map(|map| map.operator_config).collect() };

        let logged = vec!["elastic-system".to_string()];
        assert_eq!(
            configs(operator_config_maps(config_maps(), &logged, &[])),
            vec!["real: true"]
        );

        let operator = InNamespace {
            namespace: "elastic-system".to_string(),
            item: serde_json::from_value::<StatefulSet>(json!({
                "metadata": {"name": "elastic-operator", "labels": {"control-plane": "elastic-operator"}}
            }))
            .unwrap(),
        };
        assert_eq!(
            configs(operator_config_maps(
                config_maps(),
                &[],
                std::slice::from_ref(&operator)
            )),
            vec!["real: true"]
        );
        assert!(configs(operator_config_maps(config_maps(), &[], &[])).is_empty());
    }

    #[test]
    fn source_table_streams_are_the_nine_eck_streams() {
        let mut streams: Vec<&str> = sources().iter().filter_map(|source| source.stream).collect();
        streams.sort_unstable();
        streams.dedup();
        assert_eq!(
            streams,
            vec![
                "logs-eck.operator-esdiag",
                "logs-kubernetes.event-esdiag",
                "metrics-kubernetes.container-esdiag",
                "metrics-kubernetes.node-esdiag",
                "metrics-kubernetes.pod-esdiag",
                "metrics-kubernetes.volume-esdiag",
                "settings-eck.nodeset-esdiag",
                "settings-eck.operator-esdiag",
                "settings-eck.resource-esdiag",
            ]
        );
        let mut keys: Vec<&str> = sources().iter().map(|source| source.key).collect();
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), count, "source keys must be unique");
    }

    #[test]
    fn source_table_never_reads_secrets_or_stack_diagnostics() {
        for source in sources() {
            assert_ne!(source.file, "secrets.json");
            assert!(!source.file.starts_with("elasticsearch/") && !source.file.starts_with("kibana/"));
        }
    }

    #[test]
    fn pod_effective_takes_max_of_app_sum_and_largest_init() {
        let pod: Pod = serde_json::from_value(json!({
            "spec": {
                "initContainers": [{"resources": {"requests": {"cpu": "3"}}}],
                "containers": [
                    {"resources": {"requests": {"cpu": "1", "memory": "1Gi"}, "limits": {"memory": "2Gi"}}},
                    {"resources": {"requests": {"cpu": "500m"}}}
                ]
            }
        }))
        .unwrap();
        assert_eq!(pod_effective(&pod, "cpu"), (Some(3.0), None));
        assert_eq!(
            pod_effective(&pod, "memory"),
            (Some(1_073_741_824.0), Some(2_147_483_648.0))
        );
    }
}
