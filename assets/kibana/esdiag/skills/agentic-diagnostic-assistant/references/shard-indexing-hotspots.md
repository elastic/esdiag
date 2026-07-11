# Shard Indexing Hotspots

## Purpose

Identifies hot indices and shards driving indexing load. Returns top 20 indices ranked by per-shard indexing time (both primary-only and total including replicas), shard counts per index, and per-node indexing load as a percentage of JVM uptime. Includes expert guidance on hotspot detection, remediation via increasing primary shard count or enabling logsdb mode, and node-level indexing balance assessment. Use when the user asks about indexing hotspots, write-heavy indices, indexing load imbalance, per-shard indexing time, or which indices are driving the most indexing work.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Primary Indexing Hotspots

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS indexing_time_per_shard = MAX(index.primaries.indexing.index_time_per_shard_in_millis),
primary_shards = MAX(index.primaries.shard_stats.total_count) BY index.name
| SORT indexing_time_per_shard DESC | LIMIT 20
```

### Total Indexing Hotspots

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS indexing_time_per_shard = MAX(index.total.indexing.index_time_per_shard_in_millis),
total_shards = MAX(index.total.shard_stats.total_count) BY index.name | SORT
indexing_time_per_shard DESC | LIMIT 20
```

### Node Indexing Load

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| KEEP node.name, node.tier, node.indices.indexing.index_time_in_millis, node.jvm.uptime_in_millis
| EVAL indexing_pct = ROUND(node.indices.indexing.index_time_in_millis * 100.0
/ node.jvm.uptime_in_millis, 2)
```

## Metric Guidance

- `per_shard_indexing_time`: Meaning: Normalized indexing time per shard in milliseconds. High values indicate that index/shard is a hotspot consuming disproportionate CPU. Compare primary vs total, which includes replicas, to see if replica overhead is significant. Healthy pattern: Values roughly uniform across indices. Investigation trigger: If the top 2-3 indices have dramatically higher per-shard time than the rest, those are hotspots.
- `hotspot_detection`: Common hotspot indices are high-volume data streams like traces-apm, logs-k8s, metrics-system, or any data stream with very high ingest rates concentrated on few shards. If an index has high per-shard time AND low shard count, it is severely under-sharded for its write volume.
- `remediation`: Increase number of primary shards for hotspot indices to spread write load across more shards and nodes. Consider using logsdb index mode for better compression which reduces write amplification. For data streams, adjust the index template to increase number_of_shards. If using ILM/ISM rollover, ensure rollover triggers before shards get too large.
- `node_balance`: All hot-tier nodes should show similar indexing_pct (indexing time as percentage of uptime). Uneven indexing_pct across nodes indicates shard allocation imbalance. Fix by rebalancing shards or adjusting allocation awareness settings. A node with 2-3x the indexing_pct of peers is doing disproportionate write work.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/shard-indexing-hotspots?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
