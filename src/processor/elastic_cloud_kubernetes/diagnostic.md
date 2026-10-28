An `eck-diagnostics` bundle is a platform diagnostic: operator, custom resource, and
Kubernetes state for the namespaces it was run against, plus included Elasticsearch
and Kibana diagnostics that are processed as child jobs. Only a fraction of its files
carry triage value, so the processor reads a fixed table of sources
(`bundle.rs`) through typed serde structs that act as a field allowlist. Secrets,
environment variables, annotations, `managedFields`, and user ConfigMap data never
reach a document.

Legend: ✅ processed into a stream · 🔗 enrichment/metadata only · ❌ not read.

```
✅ <ns>/{elasticsearch,kibana,agent,logstash,beat,apmserver,enterprisesearch,elasticmapsserver}.json
✅ <ns>/events.json
✅ <ns>/persistentvolumeclaims.json
✅ <ns>/pod/<operator-pod>/logs.txt
✅ <ns>/pods.json
✅ nodes.json
🔗 <ns>/configmaps.json
🔗 <ns>/persistentvolumes.json
🔗 <ns>/statefulsets.json
🔗 eck-diagnostic-errors.txt
🔗 eck-diagnostics.log
🔗 manifest.json
🔗 storageclasses.json
🔗 version.json
❌ <ns>/elasticsearch/*, <ns>/kibana/*
❌ <ns>/pod/<other-pod>/logs.txt
❌ <ns>/secrets.json
❌ <ns>/controllerrevisions.json
❌ <ns>/deployments.json, <ns>/replicasets.json
❌ <ns>/services.json, <ns>/endpoints.json
❌ <ns>/daemonsets.json
❌ <ns>/serviceaccount.json, clusterroles.txt, clusterrolebindings.txt
❌ <ns>/networkpolicies.json, podsecuritypolicies.json
```

Namespaces are the bundle's top-level directories, not the namespaces named in
`eck-diagnostics.log`, so a partial collection still processes what it captured.
A missing file is recorded in the report's processor stats and never fails the job.

#### Not read

- `<ns>/elasticsearch/*`, `<ns>/kibana/*`: included diagnostics, processed as child
  jobs whose `diagnostic.parent_id` is this bundle's `diagnostic.id`.
- Non-operator pod logs: Elastic Stack logs are unstructured, multi-format, and may
  contain user data.
- `secrets.json`: metadata only, no diagnostic value.
- `controllerrevisions.json`: large template history; revision drift is already in
  StatefulSet status.
- `deployments.json`, `replicasets.json`, `services.json`, `endpoints.json`,
  `daemonsets.json`: availability and readiness duplicate custom resource status and
  pod readiness.
- RBAC and policy dumps: RBAC failures surface as operator log errors;
  PodSecurityPolicies were removed in Kubernetes 1.25.

#### version.json, eck-diagnostics.log, eck-diagnostic-errors.txt, configmaps.json

**Datastream:** `settings-eck.operator-esdiag`

One document per bundle.
+ `kubernetes.version.{git_version,major,minor}` and `eck.diagnostics.version` from `version.json`
+ `eck.version`, `eck.operator.namespaces`, `eck.resources.namespaces` from `eck-diagnostics.log`, with the operator StatefulSet image tag as the version fallback
+ `eck.diagnostics.errors` from `eck-diagnostic-errors.txt` (collection errors are data, not a failed outcome)
+ `eck.operator.config.*` parsed from the `elastic-operator` ConfigMap's `eck.yaml`
+ `eck.license.{level,enterprise_resource_units,total_managed_memory_bytes,timestamp}` from the `elastic-licensing` ConfigMap, as numbers
+ Reads both ConfigMaps only from operator namespaces (named in `eck-diagnostics.log`, or else holding an operator-labeled StatefulSet)
- Every other ConfigMap and key

#### \<ns\>/{elasticsearch,kibana,...}.json

**Datastream:** `settings-eck.resource-esdiag`

One document per ECK custom resource.
+ `eck.resource.{kind,name}`, `spec.version`, `metadata.generation`, allowlisted scalar `status.*` (health, phase, version, node counts, `observedGeneration`, conditions, association statuses, in-progress operations)
+ Calculates `generation_lag` (`metadata.generation - status.observedGeneration`)
- Extracts `spec` beyond `version`

**Datastream:** `settings-eck.nodeset-esdiag`

One document per Elasticsearch `spec.nodeSets[]` entry.
+ `nodeset.{name,count,roles}`, `resources.{requests,limits}` for the `elasticsearch` container
+ `jvm.heap.{min,max}_bytes` from the `ES_JAVA_OPTS` value only
+ `volume_claims[].{name,storage_bytes,storage_class}`
+ Joins the `{es}-es-{nodeset}` StatefulSet for `statefulset.{replicas,ready_replicas,updated_replicas}` and `rollout_in_progress`
- Pod template env vars other than `ES_JAVA_OPTS`

#### \<ns\>/pod/\<operator-pod\>/logs.txt

**Datastream:** `logs-eck.operator-esdiag`

One document per JSON log line from pods labelled `control-plane: elastic-operator`.
+ `@timestamp`, `log.{level,logger}`, `message`, `iteration`, `took` (seconds), `error`, `controller`
+ `eck.resource.{kind,name}` from `namespace` plus the first matching `*_name` key
+ `eck.operator.{pod.name,namespace}`
- Non-JSON lines (stack traces, wrappers) are counted in `skipped_records`

#### nodes.json

**Datastream:** `metrics-kubernetes.node-esdiag`

One document per Kubernetes node.
+ `capacity.*`, `allocatable.*` parsed from Kubernetes quantities
+ `conditions[]`, `condition.*`, `taints[]`, `unschedulable`, `zone`, `roles`, `node_info.*`
+ Calculates `scheduled.{pods,requests.*,limits.*}` from the bundle's non-terminal pods

#### \<ns\>/pods.json

**Datastream:** `metrics-kubernetes.pod-esdiag`

One document per pod.
+ `status.{phase,qos_class,start_time}`, `conditions[]`, `condition.*`
+ `restart_count`, `not_ready_containers`
+ `eck.resource`, `eck.type` from owner labels; `elasticsearch.roles` from node-role labels
- Labels outside the ECK and topology allowlist

**Datastream:** `metrics-kubernetes.container-esdiag`

One document per container status, init containers included.
+ `container.{name,init,image_tag,ready,started,restart_count}`
+ `state.{current,reason,started_at,exit_code}`, `last_terminated.{reason,exit_code,finished_at}`
+ `resources.{requests,limits}` from the pod spec
- Image repository, command, args, env

#### \<ns\>/persistentvolumeclaims.json, persistentvolumes.json, storageclasses.json

**Datastream:** `metrics-kubernetes.volume-esdiag`

One document per PersistentVolumeClaim.
+ `claim.{name,phase,access_modes,storage_class,volume_mode,volume_name}`
+ `requested_bytes`, `capacity_bytes`, `resize_pending`, `conditions[]`
+ `volume.{reclaim_policy,csi_driver}` from the bound PersistentVolume
+ `storage_class.{provisioner,allow_volume_expansion,volume_binding_mode}`
+ `kubernetes.pod.name` of the pod mounting the claim
- CSI volume handles and attributes

#### \<ns\>/events.json

**Datastream:** `logs-kubernetes.event-esdiag`

One document per event. Events expire after about an hour, so this stream is often
sparse.
+ `@timestamp` from the event's last, event, or first timestamp
+ `type`, `reason`, `message`, `count`, `first_timestamp`, `last_timestamp`, `source.component`
+ `involved_object.{kind,name,namespace,field_path}`, with `kubernetes.pod` or `eck.resource` for pods and ECK kinds

Every document uses the shared envelope: `@timestamp` is the collection date unless
noted, and `diagnostic.*` carries the bundle's `diagnostic.id` with
`diagnostic.platform: elastic-cloud-kubernetes`.
