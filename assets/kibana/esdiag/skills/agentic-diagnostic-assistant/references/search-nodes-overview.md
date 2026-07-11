# Search Nodes Overview

## Purpose

Retrieves search node health metrics: per-node query and fetch totals, CPU load average (15m), query and fetch time as percentage of node uptime, and thread pool stats for search, search_worker, search_coordination, esql, and esql_worker pools (completed, rejected, queue counts). Includes expert commentary for identifying search hotspots, thread pool saturation, and load imbalances across tiers. Use when the user asks about search performance, query latency, fetch operations, search thread pool health, ES|QL thread pools, or search node balance.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Search Load By Node

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| EVAL query_pct = ROUND(node.indices.search.query_time_in_millis * 100.0 /
node.jvm.uptime_in_millis, 2), fetch_pct = ROUND(node.indices.search.fetch_time_in_millis
* 100.0 / node.jvm.uptime_in_millis, 2) | KEEP node.name, node.tier, node.os.cpu.percent,
node.indices.search.query_total, node.indices.search.fetch_total, query_pct,
fetch_pct
```

### Search Thread Pools

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| KEEP node.name, node.tier, node.os.cpu.percent, node.thread_pool.search.completed, node.thread_pool.search.rejected,
node.thread_pool.search.queue, node.thread_pool.search_worker.completed, node.thread_pool.search_worker.rejected,
node.thread_pool.search_coordination.completed, node.thread_pool.search_coordination.rejected,
node.thread_pool.esql.completed, node.thread_pool.esql.rejected
```

## Metric Guidance

- `search_thread_pools`: Search pool handles coordinating queries. search_worker handles actual shard-level work. search_coordination handles cross-node coordination. Any rejections = queue overflow, serious issue.
- `esql_threads`: ES|QL has dedicated thread pools (esql + esql_worker). Monitor for rejections separately from regular search.
- `query_pct`: Percentage of node uptime spent on queries. High values on hot nodes may indicate search load competing with indexing.
- `load_balance`: All nodes in same tier should have similar query throughput. Uneven = shard allocation issue.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/search-nodes-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
