## Context

This change is stacked on `feat/kibana-processor` (`kibana-diagnostic-processor`). The Kibana streams on that branch are:

| Stream | Source | Shape |
| --- | --- | --- |
| `metrics-kibana.node-esdiag` | `/api/stats` | One doc per node. Payload merged into `node.*` (`process`, `os`, `response_times`, `requests`, `elasticsearch_client`, `kibana`) |
| `metrics-kibana.status-esdiag` | `/api/status` | One doc per node. `name`, `uuid`, `version`, `status.{overall,core,plugins}` |
| `metrics-kibana.task_manager-esdiag` | `/api/task_manager/_health` | One doc per node. Raw response flattened at the document root |
| `metrics-kibana.{alerts,fleet,detection_engine,stack_monitoring}-esdiag` | Feature health APIs | One doc per node |
| `settings-kibana.{alerts,fleet,security,spaces,synthetics,uptime,detection_engine}-esdiag` | Feature configuration APIs | One doc per object |

There is no `logs-kibana.*` stream on the base branch. `kibana_settings` (`/api/settings`) is collected only for Kibana versions earlier than 8.0.0 and is not processed.

Every document carries the shared `KibanaMetadata` fields: `@timestamp` (the diagnostic collection time), `diagnostic.*`, `node.{id,name,version.number}`, and `data_stream.*`. Each diagnostic is a single point in time per node, so a time axis shows trends only across several diagnostics. The primary views are per-node comparisons within a diagnostic.

The only index template is `metrics-kibana.json`. It explicitly maps metadata only and relies on `dynamic: true` for payloads. `esdiag@settings` sets `index.mapping.total_fields.ignore_dynamic_beyond_limit: true`, so fields beyond the limit are **silently dropped**.

Existing packaged dashboards, such as `Logstash Pipeline Summary`, use by-value Lens panels, markdown section headers, data view references, and `ESDiag` plus product tags. Assets live in `assets/kibana/esdiag/objects/<type>/<Title>.json` and are listed in `manifest/saved_objects.json`. `esdiag setup` pushes them through the saved object sync in `src/setup.rs`.

## Goals / Non-Goals

**Goals:**
- Kibana Host Health and Kibana Task Manager dashboards, installable by `esdiag setup`.
- Document shapes and mappings that Lens can chart reliably across Kibana versions.
- A Kibana navigation links panel that later Kibana dashboards can join.

**Non-Goals:**
- Kibana server log ingestion or log dashboards. No logs stream exists yet.
- Collecting `/api/stats?extended=true` or new Kibana sources.
- Dashboards for alerts, Fleet, detection engine, security, spaces, or synthetics. These are listed as candidates under Open Questions.
- Reshaping `status.plugins.*`. Plugin status keys have the same dynamic-key problem and belong with a future status dashboard.

## Decisions

### Split Task Manager per-task-type maps into their own stream
Task Manager health keys the following by task type: `stats.workload.value.task_types.<type>`, `stats.runtime.value.drift_by_type.<type>`, `stats.runtime.value.execution.duration.<type>`, and `stats.runtime.value.execution.result_frequency_percent_as_number.<type>`. Lens cannot break down by object keys. Dynamic mapping adds roughly 15 fields per task type per section. Deployments with alerting, security, and Fleet run hundreds of task types, which pushes the index past the field limit, where `ignore_dynamic_beyond_limit` drops fields without warning.

**Decision:** `TaskManagerHealth::documents_export` emits:
1. A summary doc to `metrics-kibana.task_manager-esdiag` with the four maps removed.
2. One doc per task type to `metrics-kibana.task_manager.task_type-esdiag`, with the shape `task_type.{name, workload.{count, status.*}, drift.{p50,p90,p95,p99}, duration.{p50,p90,p95,p99}, result_frequency.{Success, RetryScheduled, Failed, status}}` plus shared metadata.

The per-type summary is attached with `ProcessorSummary::add_child`, following the `nodes_stats` child-stream pattern. `TaskManagerHealth` keeps its raw `Value` deserialization. Shaping walks the `Value` and removes the maps once they are extracted, so the response is not cloned.

The stream name matches the existing `metrics-kibana.*-esdiag*` template pattern, so no new index template pattern is needed. Explicit field mappings are added in a higher-priority `metrics-kibana.task_manager.json` template (see the next decision).

**Alternatives:**
- ES|QL panels over the raw doc. Rejected because ES|QL cannot iterate over object keys either.
- `flattened` mapping for the maps. Rejected because it stores values as keywords, so numeric aggregations are impossible.
- Keeping the maps in the summary as well. Rejected because it preserves the field explosion.

### Explicit mappings for dashboard fields
The shared `esdiag@mappings` `skip_histograms` dynamic template maps every `*histogram` object as `enabled: false`. Today `node.process.event_loop_delay_histogram` is stored but not searchable or aggregatable, so the event loop delay percentiles cannot be charted. Explicit properties take precedence over dynamic templates, so mapping `node.process.event_loop_delay_histogram.percentiles.{50,75,95,99}` as `double` fixes this without changing the shared component.

Dynamic mapping also infers `long` from the first integer. Later fractional values, such as load, drift percentiles, utilization, and percentages, are then truncated by coercion. A data view also lists only mapped fields. Lens panels that reference unmapped fields, such as cgroup fields on bare metal or `capacity_estimation` on older Kibana, show errors instead of empty results.

**Decision:** Explicitly map every dashboard-referenced field.
- Node stats fields go in `metrics-kibana.json` under `node.*`. The existing test `kibana_metrics_template_maps_dashboard_metadata_and_keeps_payload_dynamic` pins the top-level keys to `@timestamp`, `data_stream`, and `node`, which still holds. Nested `node.*` additions need no change to that test.
- Task Manager fields go in a new `metrics-kibana.task_manager.json` template. It uses pattern `metrics-kibana.task_manager*-esdiag*` at priority 310 with the same composed components, and covers both Task Manager streams. The field sets are disjoint: `stats.*` for the summary and `task_type.*` for per-type docs.
- Status fields go in a new `metrics-kibana.status.json` template at priority 310. The status document has a top-level `status` object, while Task Manager and other Kibana health documents use `status` as a string, so the status mapping cannot live in the shared `metrics-kibana.json`. Both `status.overall.level` (8.x and later) and `status.overall.state` (7.x) are mapped so the status table resolves either shape.
- Each higher-priority template repeats the `@timestamp`, `data_stream`, and `node` metadata mappings, because a data stream uses only its highest-priority template. A test keeps those copies equal to `metrics-kibana.json`.

Everything else stays dynamic, which is consistent with the base design.

### Keep `concurrent_connections` in node stats
Kibana `/api/stats` reports `concurrent_connections`, and the Host Health dashboard needs it. The 7.17, 8.19, and 9.3 fixture archives all report this field.

**Decision:** Rely on `NodeStats` flattening every version-dependent field, which the base processor already does. A fixture test pins `concurrent_connections` and the cgroup fields in the emitted document, so a future typed field cannot silently drop them.

### Dashboard composition
- **Data views** (new saved objects, `*` suffix as in the Elasticsearch views): Kibana Node Stats, Kibana Status, Kibana Task Manager, and Kibana Task Manager Task Types.
- **Links:** `Kibana Overviews` links object with Host Health and Task Manager entries, `useCurrentDateRange: true` and `useCurrentFilters: true` so a `diagnostic.id` filter carries across dashboards. The Elasticsearch links set `useCurrentFilters: false`. Kibana diagnostics are scoped per collection, so carrying filters is intentional here.
- **IDs:** `kibana-host-health` and `kibana-task-manager`. These are readable slugs, like `logstash-pipeline-summary`.
- **Tags:** `Kibana` and `ESDiag`, plus `Beta` until the dashboards have been validated against field bundles.
- **Aggregation rules:**
  - Per-node values use `last_value` or `max` by `node.name`. `@timestamp` is identical within a diagnostic, so `median` over one doc is also exact. Follow the existing Logstash panels (`median`) for consistency.
  - Cluster-scoped Task Manager fields (`stats.workload.*` and `stats.capacity_estimation.*`) use `max` and never `sum`. Every Kibana node reports the same cluster-wide workload. Filter to one `diagnostic.id` to read a single collection.
  - Heap percentage is the Lens formula `max(node.process.memory.heap.used_bytes) / max(node.process.memory.heap.size_limit)`, computed per `node.name` bucket. Elasticsearch cannot rank terms by a formula, so the summary tile breaks down by node and collapses the buckets with `max` instead of ranking the top node.
  - Event loop delay and response time tiles rank the top `node.name` by the metric, so they show which node is worst.

#### Kibana Host Health layout
1. Header markdown and links panel.
2. Metric tiles: node count (`unique_count(node.name)`), maximum heap percentage, maximum `node.process.event_loop_delay_histogram.percentiles.99`, and maximum `node.response_times.avg_ms`.
3. Node status table from the status view: version, overall level, legacy 7.x state, Elasticsearch and saved objects levels, and summary.
4. Bar charts by `node.name`:
   - heap used against size limit
   - resident set size
   - event loop delay p50, p95, and p99
   - event loop utilization
   - OS load 1m, 5m, and 15m
   - OS memory used against total
5. Traffic: response time average and maximum, requests total and disconnects, concurrent connections, and Elasticsearch client active sockets, idle sockets, and queued requests.
6. Container section: CFS quota and period, throttled periods, throttled nanoseconds, and cgroup memory current bytes.
7. Full per-node datatable, as defined in the spec.

#### Kibana Task Manager layout
1. Header markdown and links panel.
2. Status tiles: overall `status` and section statuses per node.
3. Configuration table: `stats.configuration.value.poll_interval`, `claim_strategy`, and `capacity.*`.
4. Runtime charts by node: drift, load, `polling.claim_duration`, and `polling.duration` percentiles. Table of `polling.last_successful_poll`, `claim_conflicts`, and `claim_mismatches`.
5. Capacity estimation:
   - observed Kibana instances against proposed provisioned and minimum required instances
   - maximum throughput per minute per Kibana against average required throughput
6. Workload tiles using `max`: `count`, `overdue`, `overdue_non_recurring`, and `non_recurring`.
7. Task type section:
   - top 10 task types by failed percentage
   - top 10 by p99 drift
   - full task type datatable sorted by p99 drift

### No processor type-state or trait changes
The work stays inside the existing `DocumentExporter<Lookups, KibanaMetadata>` implementation for `TaskManagerHealth` and the `NodeStats` data model. No new receiver, processor, or exporter traits and no type-state transitions are introduced. The `Kibana` processor lifecycle is unchanged.

## Risks / Trade-offs

- [Task Manager field names vary by Kibana version. For example, `claim_strategy` and `capacity.as_cost` appear only in 8.14 and later, and `capacity_estimation` in 7.16 and later.] → Explicit mappings keep the fields resolvable. Panels show no data instead of an error. Verify against 7.17, 8.x, and 9.x responses.
- [Removing maps from the summary changes a document shape the base PR already emits.] → The base PR is unreleased, so no deployed indices depend on the shape. Call this out in the stacked PR description.
- [Saved objects are authored for Kibana 9.4.0 (`spaces.yml`). Older Kibana cannot import newer Lens migrations.] → This is the same constraint as the existing dashboards, so no new risk.
- [Hand-authored Lens JSON is error-prone.] → The saved objects were generated in the existing asset format, installed with `esdiag setup` into a Kibana 9.4 stack holding processed 7.17, 8.19, and 9.3 bundles, and rendered through Kibana reporting to confirm every panel resolves without errors.
- [`setup.rs` tests pin the saved object count (90).] → Update the counts with the new objects.

## Migration Plan

This is additive for users. Rerun `esdiag setup` to install the templates and dashboards. Kibana diagnostics that were processed before this change keep the old Task Manager summary shape, have no per-type docs, and have no `concurrent_connections`. Reprocess those bundles to populate the new panels. To roll back, revert the PR. Extra indexed fields are harmless.

## Open Questions

- Should the `Beta` tag stay until field validation, or should the dashboards ship without it?
- Candidate follow-up dashboards, based on the streams already on the base branch:
  - **Kibana Status & Plugins** (`metrics-kibana.status`): degraded or unavailable core and plugin services. This needs a per-plugin doc split like the task type split.
  - **Kibana Alerting & Rules** (`metrics-kibana.alerts` and `settings-kibana.alerts`): framework and decryption health, rules by type, enabled state, schedule interval, and last execution status. Pairs with Task Manager `alerting:*` task types.
  - **Kibana Fleet Overview** (`metrics-kibana.fleet` and `settings-kibana.fleet`): agent status counts, policies, and installed package versions.
  - **Kibana Security Detection Engine** (`metrics-kibana.detection_engine` and `settings-kibana.detection_engine`): rule execution health and failing or slow rules.
  - **Kibana Configuration Inventory** (`settings-kibana.{spaces,security,synthetics,uptime}`): spaces, roles, users, connectors, and synthetics locations.
  - **Kibana Logs**: requires a new `logs-kibana` processor and stream first.
