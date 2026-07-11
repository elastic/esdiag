# Search Hot Spotting Overview

## Purpose

Identifies search hotspots by analyzing shard-level query time and query count metrics. Returns total search stats (query time, query count, shard count), top 15 data streams by query time, and per-node search load breakdown (query time, query count, shard count). Includes expert guidance on detecting query time concentration, node imbalance, expensive vs cheap query patterns, and remediation (add replicas, optimize queries, force-merge read-only indices). Use when the user asks about search performance, search hotspots, slow queries, query load distribution, or search node balance.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Search Totals

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS total_query_time = SUM(shard.search.query_time_in_millis), total_query_count
= SUM(shard.search.query_total), total_shards = COUNT(*)
```

### Search By Datastream

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND index.data_stream.name IS NOT NULL | STATS query_time = SUM(shard.search.query_time_in_millis),
query_count = SUM(shard.search.query_total) BY index.data_stream.name | SORT
query_time DESC | LIMIT 15
```

### Search By Node

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS query_time = SUM(shard.search.query_time_in_millis), query_count = SUM(shard.search.query_total),
shard_count = COUNT(*) BY node.name
```

## Metric Guidance

- `query_time_concentration`: If top 2-3 data streams account for most query time, those are search hotspots. Could be detection rules, dashboards, or ML jobs targeting those indices.
- `node_balance`: Query time should be roughly proportional to shard count per node. If one node has much higher query time per shard, it may have slower hardware or more expensive shards.
- `query_count_vs_time`: High count with low time = many cheap queries (normal). Low count with high time = few expensive queries (investigate query patterns).
- `remediation`: Add replicas to spread search load. Optimize expensive queries. Consider force-merging to single segment for read-only indices.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/search-hot-spotting-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
