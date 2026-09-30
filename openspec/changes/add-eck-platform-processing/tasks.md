## 1. Fixture and receiver primitives

- [ ] 1.1 Hand-author a synthetic ECK bundle directory under `tests/archives/` with fictional names: root `manifest.json`, `version.json`, `eck-diagnostics.log`, `eck-diagnostic-errors.txt`, `nodes.json`, `storageclasses.json`, and two namespaces covering an operator pod with ECS JSON + non-JSON log lines, an Elasticsearch CR with two nodeSets (one StatefulSet mid-rollout), Kibana/Agent CRs, an empty `beat.json`, a Pending pod without container statuses, an OOMKilled container, a PVC with pending resize, Warning events, `elastic-operator`/`elastic-licensing` ConfigMaps, a user ConfigMap, and a container env var with a plaintext value
- [ ] 1.2 Add `Receiver::list_bundle_dirs` for archive and directory receivers, erroring for API receivers like `read_bundle_json`
- [ ] 1.3 Add `Receiver::read_bundle_lines` returning a line stream for archive (buffered zip entry reader on a blocking task) and directory receivers
- [ ] 1.4 Unit-test both primitives against a directory and a zipped copy of the fixture, including a missing path

## 2. Parsing building blocks

- [ ] 2.1 Add `elastic_cloud_kubernetes/quantity.rs` parsing CPU to cores and memory/storage to bytes (SI, binary, milli, exponent, plain); test the spec's representative values and an invalid quantity
- [ ] 2.2 Add minimal serde structs for the kubectl `List` envelope and each processed/enrichment source (nodes, pods, StatefulSets, PVCs, PVs, StorageClasses, events, ECK CRs, ConfigMaps with allowlisted `data`), all fields optional
- [ ] 2.3 Add the Elastic-owned label key predicate and `ES_JAVA_OPTS` heap extraction; test that other labels and env vars are dropped
- [ ] 2.4 Add operator log line parsing (ECS keys, `*_name` → `eck.resource.{kind,name}`, non-JSON rejection) and `eck-diagnostics.log` version/namespace extraction with image-tag fallback

## 3. ECK processor

- [ ] 3.1 Add the `EckSource` table (`key`, `file`, `scope`, `stream`) and the `EckBundle<Discovered>` / `EckBundle<Indexed>` typestate with consuming `discover`, `index`, and `export` transitions
- [ ] 3.2 Build `EckLookups` in `index`: nodes, storage classes, PVs, pod→node, per-node request/limit sums, PVC→pod, StatefulSets, operator pods, operator metadata
- [ ] 3.3 Implement `DocumentExporter` for the operator summary (`settings-eck.operator-esdiag`)
- [ ] 3.4 Implement `DocumentExporter` for ECK resources (`settings-eck.resource-esdiag`) including `generation_lag`
- [ ] 3.5 Implement `DocumentExporter` for nodeSets (`settings-eck.nodeset-esdiag`) including StatefulSet rollout state
- [ ] 3.6 Implement `DocumentExporter` for Kubernetes nodes (`metrics-kubernetes.node-esdiag`) with `scheduled.*` sums
- [ ] 3.7 Implement `DocumentExporter` for pods and containers (`metrics-kubernetes.pod-esdiag`, `metrics-kubernetes.container-esdiag`)
- [ ] 3.8 Implement `DocumentExporter` for volumes (`metrics-kubernetes.volume-esdiag`) with PV and StorageClass joins
- [ ] 3.9 Implement `DocumentExporter` for events (`logs-kubernetes.event-esdiag`) with the timestamp fallback chain
- [ ] 3.10 Implement streamed operator log export (`logs-eck.operator-esdiag`) recording the non-JSON line count in the summary
- [ ] 3.11 Replace the no-op `ElasticCloudKubernetesDiagnostic::process` with discover → index → export, sending one `ProcessorSummary` per source and tolerating missing/empty files; remove the unused `k8s_node` lookup

## 4. Assets and Kibana link

- [ ] 4.1 Add `esdiag@eck-metadata` and `esdiag@eck-mappings` component templates
- [ ] 4.2 Add the nine index templates under `assets/elasticsearch/index_templates/` and register them wherever existing templates are enumerated for setup
- [ ] 4.3 Add `KibanaLinkTarget` to `Exporter::kibana_link` / `resolved_kibana_link` and select `EckPlatform` for ECK parents; keep child and non-ECK links on the Cluster Report
- [ ] 4.4 Author the "ECK Platform Overview" dashboard and its data views under `assets/kibana/esdiag/objects/` with the panels from design §11, pinning `-esdiag` in every data view

## 5. Verification

- [ ] 5.1 Add an archive processing test over the fixture asserting per-stream document counts, the parent/child `diagnostic.uuid`/`parent_id` join, `OOMKilled`, the Pending pod, `rollout_in_progress`, the pending resize, numeric licensing values, and the operator log non-JSON count
- [ ] 5.2 Add an allowlist test asserting no emitted document contains the fixture's env var name/value, user ConfigMap data, annotations, `managedFields`, or any `secrets.json` content
- [ ] 5.3 Add a Kibana link test asserting ECK parents target the ECK dashboard and Elasticsearch children target the Cluster Report
- [ ] 5.4 Run `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test`, including the stream/template drift and saved-object data view tests
- [ ] 5.5 Smoke run `esdiag process` on the fixture into a local stack (`esdiag setup` first), open the ECK parent's Kibana link, and confirm every dashboard panel renders fixture data
- [ ] 5.6 Smoke run `esdiag process` on a real ECK bundle locally without committing or recording any of its data or path, and confirm the parent and child jobs complete

## 6. Docs

- [ ] 6.1 Add `src/processor/elastic_cloud_kubernetes/diagnostic.md` with the file inventory and stream catalog from the design
- [ ] 6.2 Update ECK mentions in `docs/` (processing support) and `CONTEXT.md` if its ECK description changes
- [ ] 6.3 Add a `CHANGELOG.md` entry under Unreleased following `.agents/skills/changelog/SKILL.md`, referencing [elastic/esdiag#73](https://github.com/elastic/esdiag/issues/73)
