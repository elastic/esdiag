## ADDED Requirements

### Requirement: ECK platform data is processed alongside included diagnostics
The ECK diagnostic processor SHALL index the curated ECK platform files of a Loaded
ECK bundle in addition to fanning out its included diagnostics. Platform documents
SHALL carry `diagnostic.platform` = `ECK` and no `diagnostic.application`, and their
`diagnostic.uuid` SHALL equal the `diagnostic.parent_id` of the bundle's included child
diagnostics. Included child processing SHALL be unchanged.

#### Scenario: ECK bundle produces platform and child documents
- **GIVEN** an ECK bundle whose manifest lists an included Elasticsearch diagnostic
- **WHEN** the bundle is processed
- **THEN** documents MUST be written to the `*-eck.*-esdiag` and `*-kubernetes.*-esdiag` streams for the parent
- **AND** the included Elasticsearch diagnostic MUST still be processed as a child job

#### Scenario: Parent and child documents are joinable
- **WHEN** an ECK parent and its Elasticsearch child are processed
- **THEN** every ECK platform document's `diagnostic.uuid` MUST equal the child documents' `diagnostic.parent_id`

#### Scenario: Platform documents have no application
- **WHEN** an ECK platform document is emitted
- **THEN** `diagnostic.platform` MUST be `ECK`
- **AND** `diagnostic.application` MUST be absent

### Requirement: Bundle layout is discovered, not assumed
The processor SHALL treat root-level files as cluster-scoped and each root-level
directory containing at least one recognized namespaced resource file as a namespace.
Nested Stack diagnostic directories (`<namespace>/elasticsearch/*`,
`<namespace>/kibana/*`, and other included diagnostic paths) SHALL NOT be read by the
platform processor. A missing file, a file whose list is empty, or a namespace with no
pods SHALL produce no documents for that source and SHALL NOT fail processing.

#### Scenario: Multiple namespaces
- **GIVEN** a bundle with two namespace directories each containing `pods.json`
- **WHEN** the bundle is processed
- **THEN** pod documents MUST be emitted for both namespaces with the correct `kubernetes.namespace`

#### Scenario: Empty custom resource list
- **GIVEN** a namespace whose `beat.json` is a `List` with no items
- **WHEN** the bundle is processed
- **THEN** no `settings-eck.resource-esdiag` document MUST be emitted for Beats
- **AND** the diagnostic outcome MUST NOT be degraded by that file

#### Scenario: Unavailable API file
- **GIVEN** the bundle lacks `podsecuritypolicies.json` or it is empty because the API does not exist on the cluster
- **WHEN** the bundle is processed
- **THEN** processing MUST continue without error for that file

### Requirement: Operator summary document
The processor SHALL emit exactly one `settings-eck.operator-esdiag` document per ECK
bundle containing: the ECK operator version, the `eck-diagnostics` tool version, the
Kubernetes server version (`gitVersion`, `major`, `minor`), the operator and resource
namespaces, the parsed operator configuration from the `elastic-operator` ConfigMap
`eck.yaml` key, the ECK licensing values from the `elastic-licensing` ConfigMap
(license level, enterprise resource units, total managed memory in bytes, timestamp),
and the contents of `eck-diagnostic-errors.txt` when non-empty. The ECK version SHALL
be read from `eck-diagnostics.log` and SHALL fall back to the operator container image
tag when the log does not state it.

#### Scenario: Licensing values are numeric
- **GIVEN** the `elastic-licensing` ConfigMap stores `enterprise_resource_units` and `total_managed_memory_bytes` as strings
- **WHEN** the operator document is emitted
- **THEN** both values MUST be indexed as numbers

#### Scenario: ECK version fallback
- **GIVEN** `eck-diagnostics.log` is missing or has no ECK version line
- **AND** the operator StatefulSet container image is tagged with a version
- **WHEN** the operator document is emitted
- **THEN** the ECK version MUST be taken from the image tag

#### Scenario: Missing licensing ConfigMap
- **GIVEN** no `elastic-licensing` ConfigMap is present in any collected namespace
- **WHEN** the operator document is emitted
- **THEN** the document MUST omit the licensing fields rather than failing

### Requirement: ECK custom resource documents
The processor SHALL emit one `settings-eck.resource-esdiag` document per item in each
ECK custom resource list (`elasticsearch.json`, `kibana.json`, `agent.json`,
`logstash.json`, `beat.json`, `apmserver.json`, `enterprisesearch.json`,
`elasticmapsserver.json`, and any additional ECK resource list the processor
recognizes). Each document SHALL contain the resource kind, name, namespace,
`spec.version`, `metadata.generation`, and the full `status` object as reported by the
operator (health, phase, expected/available nodes, association statuses,
`observedGeneration`, conditions). It SHALL also contain `generation_lag` =
`metadata.generation - status.observedGeneration` when both are present.

#### Scenario: Unhealthy Elasticsearch resource is visible
- **GIVEN** an Elasticsearch resource whose status reports health `yellow` and phase `ApplyingChanges`
- **WHEN** the resource document is emitted
- **THEN** the document MUST carry those values in `status.health` and `status.phase`

#### Scenario: Operator has not observed the latest spec
- **GIVEN** a resource with `metadata.generation` 5 and `status.observedGeneration` 4
- **WHEN** the resource document is emitted
- **THEN** `generation_lag` MUST be 1

### Requirement: Elasticsearch nodeSet documents
The processor SHALL emit one `settings-eck.nodeset-esdiag` document per
`spec.nodeSets[]` entry of each Elasticsearch resource containing: the owning
Elasticsearch name, nodeSet name, `count`, `node.roles` from the nodeSet config, the
`elasticsearch` container resource requests and limits (parsed to numbers), JVM heap
`-Xms`/`-Xmx` when set in `ES_JAVA_OPTS` (parsed to bytes), and each volume claim
template's storage request (bytes) and storage class. It SHALL join the StatefulSet
named for that nodeSet and include `replicas`, `readyReplicas`, `updatedReplicas`, and
`rollout_in_progress` = `currentRevision != updateRevision`.

#### Scenario: Stuck rolling upgrade
- **GIVEN** a nodeSet whose StatefulSet has `currentRevision` different from `updateRevision`
- **WHEN** the nodeSet document is emitted
- **THEN** `rollout_in_progress` MUST be `true`

#### Scenario: Heap only recorded when explicit
- **GIVEN** a nodeSet whose pod template sets no `ES_JAVA_OPTS`
- **WHEN** the nodeSet document is emitted
- **THEN** the heap fields MUST be absent

### Requirement: Kubernetes node documents
The processor SHALL emit one `metrics-kubernetes.node-esdiag` document per item of the
root `nodes.json` containing: node name, `capacity` and `allocatable` for cpu (cores),
memory, ephemeral storage (bytes) and pods, each `status.conditions` type/status,
`spec.taints`, `spec.unschedulable`, the `topology.kubernetes.io/zone` and
`node-role.kubernetes.io/*` labels, and `status.nodeInfo` versions. It SHALL add
`scheduled.pods`, `scheduled.requests.cpu`, `scheduled.requests.memory`,
`scheduled.limits.cpu`, and `scheduled.limits.memory` summed over the collected pods
bound to that node. Node `images` and `addresses` SHALL NOT be indexed.

#### Scenario: Memory pressure is queryable
- **GIVEN** a node whose `MemoryPressure` condition is `True`
- **WHEN** the node document is emitted
- **THEN** the condition MUST be queryable by type and status

#### Scenario: Requests are summed from collected pods
- **GIVEN** two collected pods on node N requesting `2` and `500m` CPU
- **WHEN** node N's document is emitted
- **THEN** `scheduled.requests.cpu` MUST be `2.5` and `scheduled.pods` MUST be `2`

### Requirement: Kubernetes pod and container documents
The processor SHALL emit one `metrics-kubernetes.pod-esdiag` document per pod in each
collected namespace, including pods with no container statuses, containing: name,
namespace, `status.phase`, `status.qosClass`, `status.conditions`, `spec.nodeName`,
`status.startTime`, the owning ECK resource kind and name, the ECK resource type
(`common.k8s.elastic.co/type`), Elasticsearch roles from `elasticsearch.k8s.elastic.co/node-*`
labels, the total restart count, and the number of non-ready containers. It SHALL emit
one `metrics-kubernetes.container-esdiag` document per entry of
`status.initContainerStatuses` and `status.containerStatuses` containing the pod
identity fields plus container name, init flag, image tag, `ready`, `started`,
`restartCount`, current state and reason, last terminated reason, exit code and
finished time, and that container's resource requests and limits (parsed to numbers).

#### Scenario: OOMKilled container is visible
- **GIVEN** a container whose `lastState.terminated.reason` is `OOMKilled`
- **WHEN** the container document is emitted
- **THEN** the document MUST carry `OOMKilled` as its last terminated reason with its exit code and restart count

#### Scenario: Pending pod without container statuses
- **GIVEN** a pod in phase `Pending` with `PodScheduled` = `False` and no `containerStatuses`
- **WHEN** the bundle is processed
- **THEN** a pod document MUST be emitted with that phase and condition
- **AND** no container documents MUST be emitted for it

#### Scenario: Elasticsearch pod joins its Elasticsearch node
- **WHEN** a pod document is emitted for an Elasticsearch pod
- **THEN** its `kubernetes.pod.name` MUST equal the Elasticsearch `node.name` reported by the included Elasticsearch diagnostic

### Requirement: Kubernetes volume documents
The processor SHALL emit one `metrics-kubernetes.volume-esdiag` document per
PersistentVolumeClaim in each collected namespace containing: claim name, namespace,
phase, access modes, storage class, requested storage (bytes), provisioned
`status.capacity` storage (bytes), `status.conditions`, and the pod mounting it. It
SHALL join the bound PersistentVolume (reclaim policy, CSI driver) and the
StorageClass (provisioner, `allowVolumeExpansion`, `volumeBindingMode`).
PersistentVolumes not bound to a collected claim SHALL NOT be emitted.

#### Scenario: Pending volume expansion
- **GIVEN** a claim requesting 20Gi whose `status.capacity` is 10Gi
- **WHEN** the volume document is emitted
- **THEN** requested and provisioned bytes MUST differ so the pending resize is queryable

### Requirement: Kubernetes event documents
The processor SHALL emit one `logs-kubernetes.event-esdiag` document per event in
each collected namespace containing: type, reason, message, count, first and last
timestamps, reporting component, and involved object kind, name, and namespace.
`@timestamp` SHALL be the event's `lastTimestamp`, else `eventTime`, else
`firstTimestamp`, else the diagnostic collection date.

#### Scenario: Warning events are filterable
- **GIVEN** an event of type `Warning` with reason `FailedScheduling`
- **WHEN** the event document is emitted
- **THEN** it MUST be filterable by type and reason and carry the involved pod name

### Requirement: Operator log documents
The processor SHALL identify operator pods by the `control-plane: elastic-operator`
label, read each operator pod's `pod/<pod>/logs.txt` line by line without loading the
file into memory, and emit one `logs-eck.operator-esdiag` document per JSON log line
containing `@timestamp`, `log.level`, `log.logger`, `message`, `namespace`,
`iteration`, `took`, `error`, and the reconciled resource normalized to
`eck.resource.kind` and `eck.resource.name` from whichever `*_name` field the line
carries. Lines that are not JSON objects SHALL NOT be indexed; their count SHALL be
recorded in the processor summary.

#### Scenario: Reconcile duration is numeric
- **GIVEN** an operator log line with `took` and `es_name`
- **WHEN** the log document is emitted
- **THEN** `took` MUST be indexed as a number
- **AND** `eck.resource.kind` MUST be `Elasticsearch` with `eck.resource.name` from `es_name`

#### Scenario: Non-JSON line
- **GIVEN** an operator log containing a non-JSON line
- **WHEN** the log is processed
- **THEN** that line MUST NOT produce a document and processing MUST continue

#### Scenario: Stack pod logs are not read
- **WHEN** an ECK bundle is processed
- **THEN** `pod/<pod>/logs.txt` files of pods that are not operator pods MUST NOT be read

### Requirement: Kubernetes quantities are parsed to numbers
Every Kubernetes resource quantity indexed by the processor SHALL be parsed to a
number: CPU to cores and memory/storage to bytes, supporting decimal SI suffixes
(`k`, `M`, `G`, `T`, `P`, `E`), binary suffixes (`Ki`, `Mi`, `Gi`, `Ti`, `Pi`, `Ei`),
the milli suffix (`m`), decimal exponents, and plain numbers. An unparseable quantity
SHALL be omitted from the document and SHALL NOT fail processing.

#### Scenario: Parse representative quantities
- **WHEN** quantities `500m`, `4`, `16380820Ki`, `10Gi`, `1e3`, and `18230014742` are parsed
- **THEN** the results MUST be `0.5`, `4`, `16773959680`, `10737418240`, `1000`, and `18230014742` respectively

#### Scenario: Unparseable quantity
- **GIVEN** a container limit with an invalid quantity string
- **WHEN** the container document is emitted
- **THEN** that limit field MUST be absent and the document MUST still be emitted

### Requirement: Indexed fields are allowlisted
The processor SHALL build documents from an explicit field allowlist. It SHALL NOT
index container environment variables (other than heap values parsed from
`ES_JAVA_OPTS`), annotations, `managedFields`, labels outside the Elastic-owned label
keys (`*.k8s.elastic.co/*`, `control-plane`, `statefulset.kubernetes.io/pod-name`,
`topology.kubernetes.io/zone`, `node-role.kubernetes.io/*`), the data of any Secret,
or the data of any ConfigMap other than the `elastic-operator` `eck.yaml` key and the
`elastic-licensing` values.

#### Scenario: Plaintext env value is not indexed
- **GIVEN** a pod container with an environment variable holding a plaintext credential
- **WHEN** the pod and container documents are emitted
- **THEN** neither document MUST contain the variable name or value

#### Scenario: User ConfigMap is not indexed
- **GIVEN** a collected namespace containing a user ConfigMap with arbitrary data
- **WHEN** the bundle is processed
- **THEN** no document MUST contain that ConfigMap's data

#### Scenario: Secrets are not read into documents
- **WHEN** the bundle is processed
- **THEN** no document MUST be emitted from `secrets.json`

### Requirement: Kubernetes join keys
The processor SHALL add Kubernetes join keys: every `*-kubernetes.*-esdiag` and
`*-eck.*-esdiag` document that concerns a namespaced object SHALL carry
`kubernetes.namespace`; pod, container, volume, and event
documents SHALL carry `kubernetes.pod.name` when a pod is involved; and pod, container,
and node documents SHALL carry `kubernetes.node.name`. `kubernetes.node.name` SHALL use
the same value space as the Elasticsearch `node.attributes.k8s_node_name` attribute used
by existing Kubernetes dashboards.

#### Scenario: Container document carries join keys
- **WHEN** a container document is emitted for a scheduled pod
- **THEN** it MUST carry `kubernetes.namespace`, `kubernetes.pod.name`, and `kubernetes.node.name`

### Requirement: ECK stream templates and dashboard
Each new stream SHALL have a matching ESDiag-owned index template that passes the
existing stream/template drift test. The system SHALL ship an "ECK Platform Overview"
dashboard whose data views pin the `-esdiag` suffix, and the Kibana link reported for
an ECK parent diagnostic SHALL open that dashboard filtered to the parent's
`diagnostic.id`. Child diagnostic links SHALL be unchanged.

#### Scenario: Templates cover every ECK stream
- **WHEN** the stream/template drift test runs
- **THEN** each of the nine ECK and Kubernetes streams MUST match an index template and no new template MUST be unmatched

#### Scenario: ECK parent link opens ECK dashboard
- **GIVEN** a Kibana base URL is configured
- **WHEN** an ECK parent diagnostic completes
- **THEN** its Kibana link MUST target the ECK Platform Overview dashboard filtered by its `diagnostic.id`
- **AND** each included Elasticsearch child link MUST still target the Elasticsearch Cluster Report
