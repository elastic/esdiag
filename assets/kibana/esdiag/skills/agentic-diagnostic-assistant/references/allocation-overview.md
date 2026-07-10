# Allocation Overview

## Purpose

Retrieves shard allocation overview per node per tier: shard counts and distribution, undesired shards (rebalancing status), disk usage percentages and forecasted disk usage, forecasted ingest load, and primary vs replica shard distribution by tier. Includes expert commentary for identifying allocation imbalances, disk watermark risks, write hotspots, and rebalancing issues. Use when the user asks about shard allocation, disk usage, node balance, shard distribution, undesired shards, or allocation problems.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Allocation By Node

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| KEEP node.name, node.tier, node.allocations.shards, node.allocations.undesired_shards,
node.allocations.forecasted_disk_usage_in_bytes, node.allocations.forecasted_ingest_load,
node.fs.total.total_in_bytes, node.fs.total.used_percent, node.indices.shard_stats.total_count
```

### Allocation Stats By Tier

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS node_count = COUNT(*), total_shards = SUM(node.allocations.shards),
total_undesired = SUM(node.allocations.undesired_shards), avg_disk_pct = AVG(node.fs.total.used_percent),
max_disk_pct = MAX(node.fs.total.used_percent), avg_shards = AVG(node.indices.shard_stats.total_count)
BY node.tier
```

### Shard Primary Distribution

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS shard_count = COUNT(*), primary_count = SUM(CASE(shard.routing.primary
== true, 1, 0)) BY node.tier
```

## Metric Guidance

- `shard_allocation`: Shards should be evenly distributed across nodes in same tier. Uneven distribution = potential hotspot. Compare node.allocations.shards across nodes in same tier. Large variance means some nodes handle disproportionate load.
- `undesired_shards`: Count of shards the allocator wants to move but hasn't yet. >0 means rebalancing is in progress or blocked. Persistent undesired shards indicate allocation constraints (disk watermarks, filter rules, awareness attributes).
- `disk_usage`: Hot tier should stay <85%%. Frozen tier at 95-98%% is normal (shared cache, not real data). Watch for nodes approaching high watermark (85%%) or flood stage (95%%). At flood stage, ES sets indices to read-only, causing write failures.
- `forecasted_disk`: Predicted disk usage after pending operations complete. If forecasted > actual, expect growth from incoming shard relocations or snapshot restores. If forecasted < actual, shards are being moved away from this node.
- `forecasted_ingest_load`: Predicted write load per node based on shard assignment. Uneven values across nodes in same tier = write hotspot. Driven by which data streams have high indexing rates and where their shards land.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/allocation-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
