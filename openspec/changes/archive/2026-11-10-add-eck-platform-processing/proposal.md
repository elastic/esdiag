## Why

ECK diagnostic bundles are loaded today only to fan out into their included
Elasticsearch/Kibana diagnostics; the platform processor logs "only process included
Elasticsearch bundles" and indexes nothing about Kubernetes or the operator
([elastic/esdiag#73](https://github.com/elastic/esdiag/issues/73)). The failures ECK
users actually escalate — pods restarting or OOMKilled, pods stuck Pending, nodes under
pressure, a stuck rolling upgrade, volumes that cannot expand, operator reconcile
errors, license/ERU questions — are only visible in the platform data, so support
engineers still unzip the bundle and read raw `kubectl` JSON.

## What Changes

- **Target product**: Elastic Cloud on Kubernetes (ECK) platform diagnostics
  (`platform: ECK`, `application: None`), produced by the `eck-diagnostics` tool and
  acquired via `Load`. Included Elasticsearch/Kibana child processing is unchanged.
- Replace the no-op ECK `process` with a typed processor that indexes a curated subset
  of the bundle's platform files into nine new ESDiag-owned data streams:
  - `settings-eck.operator-esdiag` — one doc per bundle: ECK version, Kubernetes
    server version, diagnostics tool version, operator/resource namespaces, operator
    configuration (`eck.yaml`), ECK licensing (level, enterprise resource units,
    managed memory), and collection errors.
  - `settings-eck.resource-esdiag` — one doc per ECK custom resource
    (Elasticsearch, Kibana, Agent, Logstash, Beat, APM Server, Enterprise Search,
    Elastic Maps Server): version, health, phase, expected vs available nodes,
    association status, and conditions.
  - `settings-eck.nodeset-esdiag` — one doc per Elasticsearch `nodeSet`: count, roles,
    resource requests/limits, volume claim size and storage class, joined with its
    StatefulSet rollout state.
  - `logs-eck.operator-esdiag` — ECS JSON operator log lines (level, logger, reconciled
    resource, reconcile duration, error).
  - `metrics-kubernetes.node-esdiag` — one doc per Kubernetes node: capacity,
    allocatable, pressure conditions, taints, zone, versions, and the requests of
    collected pods scheduled on it.
  - `metrics-kubernetes.pod-esdiag` — one doc per pod: phase, conditions, QoS,
    scheduling node, owning ECK resource, Elasticsearch roles, restart totals.
  - `metrics-kubernetes.container-esdiag` — one doc per container status: ready,
    state, restart count, last termination reason/exit code (e.g. `OOMKilled`),
    resources.
  - `metrics-kubernetes.volume-esdiag` — one doc per PersistentVolumeClaim joined
    with its PersistentVolume and StorageClass: requested vs provisioned capacity,
    resize conditions, reclaim policy, expansion support.
  - `logs-kubernetes.event-esdiag` — one doc per Kubernetes event (Warning/Normal,
    reason, message, count, involved object).
- Deliberately **not** processed: Stack pod logs (tens of MB each; duplicate the
  application diagnostics), Secrets, ConfigMaps other than the operator and licensing
  maps, ControllerRevisions, ReplicaSets, Deployments, Services, Endpoints, RBAC
  text dumps, ServiceAccounts, NetworkPolicies, PodSecurityPolicies. These files are
  used for enrichment or skipped with rationale recorded in design.
- An explicit field allowlist: container environment values, annotations, arbitrary
  labels, `managedFields`, and ConfigMap/Secret payloads are never indexed.
- New index templates and an `esdiag@eck-*` component template pair, reconciled by
  the existing stream/template drift test.
- An "ECK Platform Overview" Kibana dashboard, and the ECK parent diagnostic's Kibana
  link targets it instead of the Elasticsearch Cluster Report.
- New bundle-receiver primitives: list the bundle's directories and stream a bundle
  text file line by line (archive and directory receivers).

## Capabilities

### New Capabilities

- `eck-platform-processing`: Which ECK bundle files are processed, the documents and
  data streams they produce, cross-file enrichment, the field allowlist, tolerance of
  missing/empty files, and the ECK dashboard link.

### Modified Capabilities

_None._ The new streams follow the existing `indexed-data-model` naming contract and
drift test unchanged; `included-diagnostic-jobs` child fan-out behavior is unchanged.

## Impact

- **Surface**: core processing logic (new ECK processor modules, receiver
  primitives), Elasticsearch assets (templates), Kibana assets (dashboard, data
  views), and the Kibana link in CLI/Web UI/API outcomes for ECK parents. No CLI
  flags or Web UI controls change.
- **Code**: `src/processor/elastic_cloud_kubernetes/`, `src/receiver/{mod,archive,directory}.rs`,
  `src/exporter/mod.rs` (dashboard link target), `assets/elasticsearch/index_templates/`,
  `assets/elasticsearch/component_templates/`, `assets/kibana/esdiag/objects/`.
- **Dependencies**: none new (`yaml_serde`, `serde_json`, `zip` already present).
- **Compatibility**: additive; new streams only. ECK bundles that previously produced
  only child documents now also produce parent documents.
- **Docs**: `CHANGELOG.md`, `src/processor/elastic_cloud_kubernetes/diagnostic.md`
  (file inventory, following the Logstash precedent).
