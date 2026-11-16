## Why

The Kibana processor (`kibana-diagnostic-processor`) indexes Kibana diagnostics into `metrics-kibana.*-esdiag` and `settings-kibana.*-esdiag`, but ESDiag ships no Kibana dashboards. Users must build their own Lens panels before they can triage Kibana resource pressure or Task Manager backlogs. This change is stacked on that processor work and gives Kibana the same packaged dashboards that Elasticsearch and Logstash already have.

## What Changes

- Add a **Kibana Host Health** dashboard. It covers per-node system resources from `metrics-kibana.node-esdiag`: OS load, OS and cgroup memory, cgroup CPU throttling, heap and RSS, event loop delay and utilization, HTTP response times, request and disconnect counts, concurrent connections, and Elasticsearch client sockets. It also shows node status from `metrics-kibana.status-esdiag`.
- Add a **Kibana Task Manager** dashboard. It covers health status, configuration, polling and claim performance, drift, load, capacity estimation, workload, and per-task-type drift, duration, and success or failure rates from Task Manager health.
- Emit one document per task type to a new `metrics-kibana.task_manager.task_type-esdiag` stream. Remove the per-task-type maps from the Task Manager summary document. Lens cannot chart values that are keyed by object property names, and those dynamic keys expand the summary mapping without limit.
- Keep `concurrent_connections` from `/api/stats` in node stats documents, and pin it with a fixture test.
- Add explicit numeric mappings for every field the dashboards use, so data views resolve those fields even when a Kibana version or environment omits them.
- Add Kibana data views, a Kibana navigation links panel, and dashboard saved objects in the `esdiag` space. Install them with the existing `esdiag setup` saved object sync.

Target product: **Kibana**. Affected areas: core processing logic (Kibana node stats and Task Manager processors), Elasticsearch index templates, and Kibana saved object assets installed by `esdiag setup`. There are no CLI flag or Web UI changes.

## Capabilities

### New Capabilities
- `kibana-dashboards`: Packaged Kibana dashboards, data views, and navigation for processed Kibana diagnostics. Starts with Kibana Host Health and Kibana Task Manager.

### Modified Capabilities
- `kibana-task-manager`: Emit per-task-type documents and keep per-task-type maps out of the summary document. This delta is against the unarchived `kibana-diagnostic-processor` change this PR stacks on.
- `kibana-node-stats`: Preserve `concurrent_connections` and map the host health dashboard fields explicitly. This delta is also against the unarchived base change.

## Impact

- `src/processor/kibana/task_manager/`: Task Manager document shaping and the new per-task-type stream.
- `src/processor/kibana/node_stats/data.rs`: Node stats field model.
- `assets/elasticsearch/index_templates/metrics-kibana.json`, plus new `metrics-kibana.task_manager.json` and `metrics-kibana.status.json` templates: explicit dashboard field mappings.
- `assets/kibana/esdiag/objects/{dashboard,index-pattern,links}/` and `assets/kibana/esdiag/manifest/saved_objects.json`: new saved objects.
- `src/setup.rs` and `src/setup/serverless_tests.rs` tests: saved object and template count assertions.
- `docs/` and `CHANGELOG.md`: document the new dashboards and stream.
