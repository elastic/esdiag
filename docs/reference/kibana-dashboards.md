---
type: Reference
title: Kibana dashboards
description: Packaged dashboards, data views, and data streams for processed Kibana diagnostics.
tags: [kibana, dashboards, data-streams]
---

# Kibana dashboards

`esdiag setup` installs these dashboards in the `esdiag` Kibana space. They are
tagged `Kibana`, `ESDiag`, and `Beta`. The `Kibana Overviews` links panel at
the top of each dashboard keeps the current time range and filters when you
switch dashboards, so a `diagnostic.id` or `node.name` filter follows you.

Each Kibana diagnostic records a single point in time for one Kibana node.
Panels therefore compare nodes, or several diagnostics of the same node, rather
than plotting trends.

## Kibana Host Health

Data views: `Kibana Node Stats` (`metrics-kibana.node-esdiag*`) and
`Kibana Status` (`metrics-kibana.status-esdiag*`).

- Summary tiles for node count, the node with the highest heap use, event loop
  delay p99, and average response time.
- Node status. Kibana 8.x and later report `status.overall.level`. Kibana 7.x
  reports `status.overall.state`.
- Process memory, heap used of the heap limit, event loop delay and
  utilization, OS load, and OS memory per node.
- Response times, requests, disconnects, concurrent connections, and
  Elasticsearch client sockets per node.
- Container CPU quota, throttling, and cgroup memory. Only Kibana 8.x and
  later on Linux report these values. Other nodes show no data.
- A per-node table with every metric above.

## Kibana Task Manager

Data views: `Kibana Task Manager` (`metrics-kibana.task_manager-esdiag*`) and
`Kibana Task Manager Task Types`
(`metrics-kibana.task_manager.task_type-esdiag*`).

- Overall and per-section health, and configuration such as poll interval,
  claim strategy, and capacity.
- Workload tiles. Every Kibana node reports the same cluster-wide workload, so
  the tiles use the maximum and never sum across nodes.
- Drift, load, and polling percentiles, plus claim conflicts and mismatches per
  node.
- Capacity estimation from Kibana 7.16 and later. It compares observed Kibana
  instances with proposed instances, and maximum throughput with required
  throughput.
- The top 10 task types by failed percentage and by p99 drift, and a task type
  table sorted by p99 drift.

## Task type documents

Task Manager health reports per-task-type values keyed by task type name, and
those names contain dots. ESDiag writes one document per task type to
`metrics-kibana.task_manager.task_type-esdiag`, with the fields
`task_type.name`, `task_type.workload`, `task_type.drift`,
`task_type.duration`, and `task_type.result_frequency`. The summary document
in `metrics-kibana.task_manager-esdiag` omits those per-task-type maps.

Kibana diagnostics processed before these dashboards existed have no task
type documents or concurrent connection counts. Reprocess them to fill those
panels.
