## 1. Reference data

- [x] 1.1 Collect or locate real Kibana diagnostics for 7.17, 8.x, and 9.x. Record the exact `/api/stats` and `/api/task_manager/_health` field paths used by the dashboards, including `concurrent_connections`, cgroup fields, `claim_strategy`, `capacity.*`, and `capacity_estimation`.
- [x] 1.2 Add trimmed `kibana_stats.json` and `kibana_task_manager_health.json` fixtures derived from those bundles for processor tests.

## 2. Node stats processing

- [x] 2.1 Confirm `NodeStats` in `src/processor/kibana/node_stats/data.rs` keeps `concurrent_connections` in emitted documents.
- [x] 2.2 Add a processor test proving `node.concurrent_connections` is exported to `metrics-kibana.node-esdiag`.

## 3. Task Manager processing

- [x] 3.1 In `src/processor/kibana/task_manager/processor.rs`, extract the per-task-type entries from `stats.workload.value.task_types`, `stats.runtime.value.drift_by_type`, `stats.runtime.value.execution.duration`, and `stats.runtime.value.execution.result_frequency_percent_as_number`. Merge them by task type into `task_type.{name, workload, drift, duration, result_frequency}` documents, and remove the maps from the summary `Value` without cloning the response.
- [x] 3.2 Export the per-type documents to `metrics-kibana.task_manager.task_type-esdiag` with shared Kibana metadata, and attach their summary with `ProcessorSummary::add_child`.
- [x] 3.3 Add tests for: one type present in all sections (single merged document), a type present in only some sections (absent sections omitted), no task types (summary only), and summary maps removed while other stats are retained.

## 4. Index templates

- [x] 4.1 Add explicit `node.*` mappings for every Host Health field to `assets/elasticsearch/index_templates/metrics-kibana.json`, including `node.process.event_loop_delay_histogram.percentiles.*` as `double` to override `skip_histograms`.
- [x] 4.2 Add `assets/elasticsearch/index_templates/metrics-kibana.task_manager.json` (pattern `metrics-kibana.task_manager*-esdiag*`, priority 310, same composed components and metadata properties). It explicitly maps the `stats.*` summary fields and `task_type.*` fields used by the Task Manager dashboard. Add `metrics-kibana.status.json` for the status fields.
- [x] 4.3 Confirm that `emitted_output_streams_and_index_templates_stay_in_sync` and `kibana_metrics_template_maps_dashboard_metadata_and_keeps_payload_dynamic` still pass. Extend asset template tests only where they would catch a real regression, such as the histogram percentiles no longer resolving to `double`.

## 5. Kibana saved objects

- [x] 5.1 Run `esdiag setup` against a local stack and process the reference bundles from task 1.1.
- [x] 5.2 Create data views in `assets/kibana/esdiag/objects/index-pattern/`: Kibana Node Stats, Kibana Status, Kibana Task Manager, and Kibana Task Manager Task Types. Each uses a `-esdiag*` title and `@timestamp`.
- [x] 5.3 Author the `kibana-host-health` dashboard in Kibana to match the `kibana-dashboards` spec and the design layout.
- [x] 5.4 Author the `kibana-task-manager` dashboard in Kibana to match the `kibana-dashboards` spec and the design layout. Use `max` per diagnostic for workload and capacity fields.
- [x] 5.5 Create the `Kibana Overviews` links object (current time range and filters carried) and embed it in both dashboards.
- [x] 5.6 Export the objects into `assets/kibana/esdiag/objects/{dashboard,index-pattern,links}/` with `Kibana`, `ESDiag`, and `Beta` tags. Add them to `assets/kibana/esdiag/manifest/saved_objects.json`.
- [x] 5.7 Update the saved object count assertions in `src/setup.rs` tests and the template count in `src/setup/serverless_tests.rs`.

## 6. Verification

- [x] 6.1 Run `cargo clippy --all-targets` with no new warnings.
- [x] 6.2 Run `cargo test` and confirm it passes.
- [ ] 6.3 Rerun `esdiag setup` from a clean stack, process each reference bundle, and open both dashboards. Confirm that every panel renders, that a `diagnostic.id` filter scopes all panels, and that the links panel navigates with filters kept.
- [x] 6.4 Process a bundle without cgroup metrics and a pre-8.x bundle without capacity estimation. Confirm the affected panels show no data and no errors.

## 7. Documentation

- [x] 7.1 Document the Kibana dashboards and the `metrics-kibana.task_manager.task_type-esdiag` stream in the relevant `docs/` pages.
- [x] 7.2 Add `CHANGELOG.md` entries following `.agents/skills/changelog/SKILL.md`.
