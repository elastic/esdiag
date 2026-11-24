## 1. Templates and envelope

- [ ] 1.1 Add `assets/elasticsearch/component_templates/esdiag@host.json` with the `host.*`, `elasticsearch.node.{id,name,match}`, `kubernetes.node.name`, and `sar.timezone` mappings from design Decision 9. Verify the JSON parses in `elasticsearch_asset_templates_tests`.
- [ ] 1.2 Add `assets/elasticsearch/index_templates/metrics-host.sar.json` with:
  - `index_patterns: ["metrics-host.sar-esdiag*"]` and `priority: 300`.
  - `composed_of` including `esdiag@host` but not `esdiag@node`.
  - `data_stream.dataset` = `host.sar`.
  - `index.mapping.total_fields.limit: 1500`.
  - Keyword `sar.metricset` and entity keys, plus explicit dashboard-field mappings, per Decision 9.

  Add a test asserting the field limit, that no explicitly mapped field matches a reserved dynamic-template pattern, and that no `metrics-node*` pattern matches `metrics-host.sar-esdiag`.
- [ ] 1.3 Add `ElasticsearchMetadata::for_data_stream_at(data_stream, timestamp)` sharing a builder with `for_data_stream`. Unit test: `@timestamp` equals the override and `diagnostic.collection_date` is unchanged.

## 2. Line hygiene, header, and timestamps

- [ ] 2.1 Implement `sar/clean.rs`: strip ANSI CSI sequences, `\r`, and NUL bytes. Unit tests cover SGR, cursor/erase CSI, and mixed control bytes, and check that column spacing is preserved.
- [ ] 2.2 Implement `sar/header.rs`: parse the `Linux …` header (tab/space mix, FQDN or short host, optional arch and CPU count) and disambiguate the report date (YYYY-MM-DD, A/B/Y with >12 rule, closest-on-or-before the collection date, two-digit year with year rollover, DD-Mon-YYYY). Unit tests cover every spec scenario under "Sample timestamps" and "Host identity".
- [ ] 2.3 Implement `sar/timestamp.rs`: parse the row prefix in all five forms, returning the token count, and implement `RolloverClock` with in-section midnight advance. Unit tests cover 12h, 24h, ISO+12h, ISO+24h, ISO `T`, and rollover.

## 3. Section parser

- [ ] 3.1 Implement `sar/columns.rs`: generic naming and unit rules (lowercase, `-`→`_`, `%`, `kb`, `MB`, `kB/s`, `/s`, latency), the `_value` guard for reserved dynamic-template patterns, and the alias overrides from design Decision 8. Unit tests check:
  - The spec values: `6.10` → `0.061`, `141.31` → `1.4131`, `5523262` KB → `5655820288`, `8.00` sectors/s → `4096.0`, and `zq-cnt/s` → `zq_cnt_per_sec`.
  - That no catalog or fuzzed column name produces a reserved field name.
  - That no generated field name maps to two value types across metricsets.
  - That the total generated field count plus the explicit template fields stays under 1500.
- [ ] 3.2 Implement the `sar/section.rs` catalog covering every row of the spec's "Complete section coverage" table: distinctive columns, entity columns, `Metricset`, and prefix. Unit tests use one header fixture per section (from observed output where available, otherwise from sysstat column definitions) and assert:
  - Each header is recognized as the right section.
  - `CPU`/`IFACE`/`INTR` collisions are resolved by metric columns.
  - An unrecognized header is classified as unknown.
- [ ] 3.3 Implement the `sar/parser.rs` state machine (design Decision 3), including per-section clock reset, `resume` after `LINUX RESTART`, `Average:` and localized summary skipping, `all`-vs-numbered CPU routing, interrupt-matrix transposition, USB description joining, recording of unknown headers, and `finish()` returning `NoHeader`/`NoData`. Fixture-driven unit tests cover:
  - Older and newer sysstat column sets.
  - Section reset.
  - Restart across midnight.
  - An unknown section between known ones.
  - ANSI-decorated input producing the same records as clean input.
- [ ] 3.4 Implement `sar/docs.rs` (`SarDoc` with `Metricset`, `EntityKey`, and `SarFields`) and record assembly (Decision 4): merge host-wide sections into one `summary` per timestamp, per-CPU sections per `(timestamp, cpu)`, and network traffic plus errors per `(timestamp, interface)`, leaving missing columns absent and keeping idle entities. Unit tests cover every "Complete section coverage" and "Single host data stream" scenario, including that entity documents carry no summary fields.

## 4. Host identity and node correlation

- [ ] 4.1 Derive `Lookups.node_endpoints` from the single parsed `Nodes` in `ElasticsearchDiagnostic::try_new` (id, name, host, ip, transport address, HTTP publish host/addr, `k8s_node_name`) without re-reading `nodes.json` and without changing `NodeDocument`. Unit test covers `hostname/ip:port`, `ip:port`, and IPv6 publish forms.
- [ ] 4.2 Parse listening sockets from `syscalls/ss.txt`, falling back to `syscalls/netstat.txt`, discarding wildcard and loopback addresses. Unit tests use synthetic `ss` and `netstat` fixtures, including IPv6 brackets.
- [ ] 4.3 Extract the manifest collection host (`host='…'` from `diagnostic_inputs`/`flags`) in `try_new`, ignoring `localhost` and loopback. Unit test covers present, absent, and loopback.
- [ ] 4.4 Implement `sar/host.rs` correlation in the spec order (`listen_socket`, `k8s_node_name`, `hostname`, `manifest_host`, `none`), with short-name ambiguity rejection and `kubernetes.node.name` on a `k8s_node_name` match. Unit tests cover every "Co-located Elasticsearch node correlation" scenario, including the multi-node ECE allocator and the exclusion of other allocators' nodes.

## 5. Syscall processing foundation

- [ ] 5.1 Add `assets/syscalls/sources.yml` with the `sar`, `ss`, and `netstat` entries from design Decision 1. Add the `SyscallSource` type (`deny_unknown_fields`), an embedded loader, and `syscall_source(key)` path lookup. Unit tests: the embedded catalog parses, an unknown field is rejected, and `sar` resolves to `syscalls/sar.txt` with tags `standard,support`.
- [ ] 5.2 Add `validate_syscall_registry(application)` against a per-application `SYSCALL_DISPATCH` table. Run it in the Elasticsearch `OnceLock` validation next to `validate_es_dispatch_registry`. Unit tests: a processable entry without a dispatch entry fails, a dispatch entry without a catalog entry fails, and the shipped Elasticsearch table passes.
- [ ] 5.3 Extend `ApiResolver` (Decision 1a):
  - `processing_defs` appends non-required `syscalls.<key>` options for entries listing the product, and a startup check rejects any `syscalls.<key>` equal to a REST key.
  - `default_processing_selection` adds syscall options by tag.
  - `ProcessSelection` gains `#[serde(default)] requested`, filled with the canonicalized user-supplied keys.
  - The collect-filter resolution and `validate_resolved_processing_selection` skip `syscalls.*` keys.

  Unit tests cover the "Syscall processing selection" scenarios: standard includes `syscalls.sar`; minimal omits it; it appears only in the Elasticsearch catalog as non-required; an explicit list records it in `requested`; a collect job selecting it validates; and a saved-job JSON without `requested` deserializes.
- [ ] 5.4 Implement the shared syscall runner (Decision 1b): selection check, `SyscallMode` from `requested`, bundle and file presence, a blank-file check that replays the first non-blank line, a `missing` summary named `syscalls.<key>` with a warning only in `Requested` mode, and dispatch to the processor. Unit tests with a stub processor cover every row of the Decision 7 matrix not specific to SAR: not selected, default-missing, requested-missing, not-a-bundle requested, and blank file.

## 6. Processor wiring and reporting

- [ ] 6.1 Implement `sar/mod.rs` `process_sar` as the Elasticsearch `sar` syscall dispatch entry:
  - Consume the non-empty line stream from the runner, resolving `ss`/`netstat` paths through the catalog.
  - Send single-section metricset documents as they parse, and drain the `summary`, `cpu`, and `network` maps at EOF.
  - Run one `document_channel` consumer for `metrics-host.sar-esdiag`, with a shared host envelope and a per-timestamp `MetadataValue` cache.
- [ ] 6.2 Run the Elasticsearch syscall dispatch from `ElasticsearchDiagnostic::process` in the same concurrent set as `ES_DISPATCH`, independent of `Platform`. Verify `validate_es_dispatch_registry` and `validate_syscall_registry` pass, that a default-mode API-only diagnostic records no SAR event, and that a bundle with binary sysstat files but no `sar.txt` reports SAR as missing with no warning.
- [ ] 6.3 Map `NoHeader`/`NoData` and mid-file read errors to a parsed summary with a recorded warning event (keeping documents already sent), and each unknown header to a warning event naming its columns. In `DiagnosticReport::add_single_processor_summary`, skip the success verdict only for zero-document summaries that carry a warning or error event. Unit tests:
  - A `command not found` fixture yields one warning event and no processor failure, in default and requested mode.
  - A file with one unknown section yields exported documents, a success verdict, and one warning.
  - A stream that errors mid-file keeps earlier documents and records one warning.
- [ ] 6.4 Add an integration test in `tests/integration/` with a synthetic directory bundle (`example.com` hosts, documentation-range IPs, `syscalls/sar.txt` containing every catalog section, `ss.txt`, and a two-node `nodes.json` sharing one IP on different ports). Assert:
  - Document counts per `sar.metricset` in `metrics-host.sar-esdiag`, including per-core CPU and idle devices, and that no other SAR stream exists.
  - `@timestamp` values.
  - Both nodes in `elasticsearch.node.name` with `match` = `listen_socket`.
  - No `node.*` fields.

  In the same file, cover the selection outcomes end to end with standard processing of bundles: no `syscalls/` directory gives no SAR event and success; a missing `sar.txt` with an explicit `syscalls.sar` selection gives one warning and success; a selection omitting `syscalls.sar` leaves the file unread and reports no SAR summary.
- [ ] 6.5 Verify `emitted_output_streams_and_index_templates_stay_in_sync` passes with the new stream.

## 7. Kibana assets

- [ ] 7.1 Process the synthetic all-sections bundle into the local stack, then author the `metrics-host.sar-esdiag` data view and export it to `assets/kibana/esdiag/objects/index-pattern/`. Verify that `shipped_data_views_follow_the_stream_naming_contract` and `shipped_saved_objects_only_query_provenance_names_the_templates_define` pass.
- [ ] 7.2 Author the "Host Activity (SAR)" dashboard with the panels and controls from the `kibana-dashboards` delta (every panel filtered on `sar.metricset`; series by `host.name`; controls for `diagnostic.id`, `host.name`, `elasticsearch.node.name`, `sar.cpu.id`, `sar.io.device`, `sar.network.interface`, and `sar.filesystem.name`). Export it to `assets/kibana/esdiag/objects/dashboard/` (plus any `lens/` objects), then verify it opens with populated panels after `esdiag setup` against the local stack.

## 8. Docs and verification

- [ ] 8.1 Update `CONTEXT.md`:
  - "Infrastructure data": host system calls come from any bundle with `syscalls/`, including ECE-allocator and Kubernetes-node collections. They are processed when selected, and SAR is host-scoped.
  - Add a "Syscall source catalog" term next to "Collection definition": `assets/syscalls/sources.yml`, keyed by `diags.yml` command keys, processing options `syscalls.<key>`.
  - Add `docs/adr/0021-syscall-sources-are-selected-per-source-not-by-platform.md`, amending ADR-0001's "`SelfManaged` yields opt-in Linux syscalls" consequence: syscall output is processed per selected source wherever a bundle contains it, and the catalog is separate from REST `sources.yml`. Link it from ADR-0001's consequence rather than editing the accepted decision.
- [ ] 8.2 Document the `syscalls.<key>` processing options, their default membership by diagnostic type, and the missing-file and failure policy in the `process` section of `docs/command-line.md`.
- [ ] 8.3 Add a "Host Activity (SAR)" section to `docs/reference/kibana-dashboards.md` covering the stream and its `sar.metricset` values, the rule to filter on `sar.metricset` before aggregating, the assumed UTC timezone, and what `elasticsearch.node.match` means.
- [ ] 8.4 Add a `CHANGELOG.md` entry following `.agents/skills/changelog/SKILL.md`, with no private issue references.
- [ ] 8.5 Run `cargo fmt --check`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace`. All must pass.
- [ ] 8.6 Run `esdiag` against the maintainer's local self-managed sample bundle (kept outside the repository; record no paths, file names, or hostnames from it). Confirm:
  - `metrics-host.sar-esdiag` receives `summary`, `cpu`, `io`, and `network` documents. The sample has no filesystem or other entity sections.
  - The counts match the file: one `summary` per sample, per-core rows for 4 CPUs, every device, and both interfaces.
  - `@timestamp` spans the SAR window.
  - The NFS, socket, and softnet fields are present on `summary` documents.
  - `elasticsearch.node.match` = `listen_socket` correlates exactly one node.
  - No unknown-section warnings are reported.
