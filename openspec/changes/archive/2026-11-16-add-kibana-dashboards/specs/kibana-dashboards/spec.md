## ADDED Requirements

### Requirement: Kibana Dashboard Installation
The system SHALL ship Kibana dashboards, data views, and navigation links as saved objects in the `esdiag` space. `esdiag setup` SHALL install them through the existing Kibana saved object sync.

#### Scenario: Setup installs Kibana dashboards
- **GIVEN** a target Kibana deployment with the `esdiag` space
- **WHEN** the user runs `esdiag setup`
- **THEN** the `kibana-host-health` and `kibana-task-manager` dashboards exist in the `esdiag` space
- **AND** each dashboard is tagged `Kibana`, `ESDiag`, and `Beta`

#### Scenario: Kibana data views are installed
- **GIVEN** `esdiag setup` has completed
- **WHEN** the user opens the data view list in the `esdiag` space
- **THEN** data views exist for `metrics-kibana.node-esdiag*`, `metrics-kibana.status-esdiag*`, `metrics-kibana.task_manager-esdiag*`, and `metrics-kibana.task_manager.task_type-esdiag*`, each using `@timestamp` as the time field

#### Scenario: Dashboards cross-link
- **GIVEN** the user is viewing either Kibana dashboard
- **WHEN** the user selects the other dashboard in the Kibana navigation links panel
- **THEN** Kibana opens that dashboard with the current time range and filters

### Requirement: Diagnostic Scoping
Every Kibana dashboard panel SHALL respect the dashboard query, filters, and time range, so that a filter on `diagnostic.id` or `node.name` scopes the whole dashboard.

#### Scenario: Filter by diagnostic
- **GIVEN** processed Kibana diagnostics from two different collections
- **WHEN** the user filters a Kibana dashboard to one `diagnostic.id`
- **THEN** every panel shows only documents from that diagnostic

### Requirement: Kibana Host Health Dashboard
The system SHALL provide a Kibana Host Health dashboard with per-node system resource metrics from `metrics-kibana.node-esdiag` and node status from `metrics-kibana.status-esdiag`.

#### Scenario: Summary tiles
- **GIVEN** node stats documents for one or more Kibana nodes
- **WHEN** the user opens the Kibana Host Health dashboard
- **THEN** it shows the Kibana node count, the highest per-node heap used percentage (`node.process.memory.heap.used_bytes` / `node.process.memory.heap.size_limit`), and the node with the highest p99 event loop delay and the node with the highest average response time

#### Scenario: Per-node resource table
- **GIVEN** node stats documents for one or more Kibana nodes
- **WHEN** the user views the node table
- **THEN** each row identifies one `node.name` with its version, OS platform, and process uptime
- **AND** each row shows its OS load (1m, 5m, 15m), OS memory used percentage, heap used and size limit, resident set size, event loop utilization, event loop delay percentiles (p50, p95, p99), response time average and maximum, total requests, disconnects, concurrent connections, and Elasticsearch client active sockets, idle sockets, and queued requests

#### Scenario: Per-node resource charts
- **GIVEN** node stats documents for multiple Kibana nodes
- **WHEN** the user views the resource charts
- **THEN** charts broken down by `node.name` show heap used compared with heap size limit, event loop delay percentiles, OS load, and OS memory used compared with total

#### Scenario: Container resource limits
- **GIVEN** node stats documents that contain cgroup metrics under `node.os.cpu` and `node.os.cgroup_memory`
- **WHEN** the user views the container panels
- **THEN** the dashboard shows the CFS quota, throttled period count, throttled time, and cgroup memory usage per node
- **AND** those panels show no data, rather than an error, for nodes that report no cgroup metrics

#### Scenario: Node status
- **GIVEN** status documents in `metrics-kibana.status-esdiag`
- **WHEN** the user views the status panel
- **THEN** the dashboard shows the Kibana version, `status.overall.level`, `status.core.elasticsearch.level`, and `status.core.savedObjects.level` for each `node.name`
- **AND** it shows `status.overall.state` for Kibana 7.x nodes, which report the legacy status shape

### Requirement: Kibana Task Manager Dashboard
The system SHALL provide a Kibana Task Manager dashboard for Task Manager health from `metrics-kibana.task_manager-esdiag` and per-task-type metrics from `metrics-kibana.task_manager.task_type-esdiag`.

#### Scenario: Health overview
- **GIVEN** Task Manager summary documents
- **WHEN** the user opens the Kibana Task Manager dashboard
- **THEN** it shows the overall `status` and the status of each stats section (configuration, workload, runtime, capacity estimation) for each node

#### Scenario: Configuration
- **GIVEN** Task Manager summary documents
- **WHEN** the user views the configuration panel
- **THEN** it shows the poll interval, claim strategy, and configured capacity for each node

#### Scenario: Runtime performance
- **GIVEN** Task Manager summary documents with runtime stats
- **WHEN** the user views the runtime panels
- **THEN** it shows the p50, p90, p95, and p99 values of drift, load, polling claim duration, and polling cycle duration for each node
- **AND** it shows the last successful poll time and claim conflict and mismatch counts

#### Scenario: Capacity estimation
- **GIVEN** Task Manager summary documents with capacity estimation stats
- **WHEN** the user views the capacity panels
- **THEN** it compares observed Kibana instances with proposed provisioned and minimum required instances
- **AND** it compares maximum throughput per minute per Kibana with the average required throughput per minute per Kibana

#### Scenario: Workload without double counting
- **GIVEN** summary documents from several Kibana nodes of one deployment in one diagnostic
- **WHEN** the user views the workload panels
- **THEN** the total task count, overdue task count, and non-recurring task count use the maximum value and never a sum across nodes

#### Scenario: Task type breakdown
- **GIVEN** per-task-type documents
- **WHEN** the user views the task type table
- **THEN** each row identifies one `task_type.name` with its workload count, workload status counts, p50 and p99 drift, p50 and p99 execution duration, and success, retry-scheduled, and failed percentages
- **AND** the table is sorted by p99 drift in descending order

#### Scenario: Problem task types
- **GIVEN** per-task-type documents
- **WHEN** the user views the problem task type charts
- **THEN** the dashboard ranks the top 10 task types by failed percentage and the top 10 by p99 drift

#### Scenario: Version gaps
- **GIVEN** a Task Manager health response from a Kibana version that omits capacity estimation
- **WHEN** the user opens the Kibana Task Manager dashboard
- **THEN** the capacity panels show empty values instead of errors, and every other panel still renders
