# Nodes Summary

## Purpose

Retrieves resource utilization summary across all nodes: per-node CPU load (15m average), JVM heap usage, disk usage, shard count, document count, dataset size, and workload split (indexing vs query time as percentage of uptime). Includes tier-level aggregations (node count, total docs, total dataset, total shards, average heap/disk/load per tier). Expert commentary covers heap thresholds (75/85/95%), disk watermarks, CPU load interpretation, and workload split analysis. Use when the user asks about node resource utilization, capacity planning, tier health, or overall cluster node status.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Node Summary

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| EVAL indexing_pct = ROUND(node.indices.indexing.index_time_in_millis * 100.0
/ node.jvm.uptime_in_millis, 2), query_pct = ROUND(node.indices.search.query_time_in_millis
* 100.0 / node.jvm.uptime_in_millis, 2) | KEEP node.name, node.tier, node.os.cpu.percent,
node.jvm.mem.heap_used_percent, node.fs.total.used_percent, node.indices.shard_stats.total_count,
node.indices.docs.count, node.indices.store.total_data_set_size_in_bytes, indexing_pct,
query_pct
```

### Tier Summary

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS node_count = COUNT(*), total_docs = SUM(node.indices.docs.count), total_dataset
= SUM(node.indices.store.total_data_set_size_in_bytes), total_shards = SUM(node.indices.shard_stats.total_count),
avg_heap_pct = AVG(node.jvm.mem.heap_used_percent), avg_disk_pct = AVG(node.fs.total.used_percent),
avg_load = AVG(node.os.cpu.percent) BY node.tier
```

## Metric Guidance

- `heap_usage`: Meaning: JVM heap utilization per node. Healthy pattern: Below 75%% is healthy. 75-85%% needs monitoring. Investigation trigger: Above 85%% risks OOM and GC pressure. Circuit breakers trigger near 95%%.
- `disk_usage`: Meaning: Filesystem utilization per node. Healthy pattern: Hot/warm tiers below 85%%. Frozen tier at 95-98%% is NORMAL (pre-reserved shared cache). Investigation trigger: High watermark at 85%% stops shard allocation. Flood stage at 95%% makes indices read-only.
- `cpu_load`: Meaning: 15-minute CPU load average as a percentage. Healthy pattern: Below 80%% sustained. Investigation trigger: load_percent.15m above 80%% sustained means the node needs more capacity or workload reduction. NOT container-aware: in Kubernetes/shared hosts, load reports host-level, making nodes appear overloaded.
- `workload_split`: Meaning: indexing_pct + query_pct shows how node CPU time is split between indexing and searching. Healthy pattern: Hot nodes should be indexing-heavy. Warm/cold nodes should be query/merge-heavy. Investigation trigger: Hot node with high query_pct = security detections or ML jobs consuming resources. Warm/cold node with high indexing_pct = unexpected write activity on read-optimized tier.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/nodes-summary?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
