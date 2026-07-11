# Ingest Nodes Overview

## Purpose

Retrieves ingest node health metrics: node counts and unique IPs by tier, configuration consistency checks (CPU/heap/version/OS variations per tier), CPU load average per node, workload breakdown (indexing/query/merge/ingest pipeline %), HTTP client connection balance, indexing pressure per node (coordinating/primary/replica with rejection counts), and key thread pool stats (write, system_write, force_merge, merge). Includes expert commentary for identifying hotspots, imbalances, and capacity issues. Use when the user asks about ingest performance, node balance, write throughput, thread pool health, or indexing pressure.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Nodes By Tier

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND node.tier IN ("hot", "warm", "ingest", "coord", "data") | STATS node_count
= COUNT(*), unique_ips = COUNT_DISTINCT(node.ip) BY node.tier
```

### Config Variations

```esql
FROM settings-node* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND node.tier IN ("hot", "warm", "ingest", "coord", "data") | STATS cpu_configs
= COUNT_DISTINCT(node.os.allocated_processors), heap_configs = COUNT_DISTINCT(node.jvm.mem.heap_max_in_bytes),
version_configs = COUNT_DISTINCT(node.version), os_configs = COUNT_DISTINCT(node.os.version)
BY node.tier
```

### Node Load Workload

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND node.tier IN ("hot", "warm", "ingest", "coord", "data") | EVAL indexing_pct
= ROUND(node.indices.indexing.index_time_in_millis * 100.0 / node.jvm.uptime_in_millis,
2), query_pct = ROUND(node.indices.search.query_time_in_millis * 100.0 / node.jvm.uptime_in_millis,
2), merge_pct = ROUND(node.indices.merges.total_time_in_millis * 100.0 / node.jvm.uptime_in_millis,
2), ingest_pct = ROUND(node.ingest.total.time_in_millis * 100.0 / node.jvm.uptime_in_millis,
2) | KEEP node.name, node.tier, node.os.cpu.percent, node.http.current_open,
node.http.total_opened, indexing_pct, query_pct, merge_pct, ingest_pct
```

### Indexing Pressure

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND node.tier IN ("hot", "warm", "ingest", "coord", "data") | KEEP node.name,
node.tier, node.indexing_pressure.memory.total.coordinating_in_bytes, node.indexing_pressure.memory.total.primary_in_bytes,
node.indexing_pressure.memory.total.replica_in_bytes, node.indexing_pressure.memory.total.coordinating_rejections,
node.indexing_pressure.memory.total.primary_rejections, node.indexing_pressure.memory.total.replica_rejections
```

### Thread Pools

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND node.tier IN ("hot", "warm", "ingest", "coord", "data") | KEEP node.name,
node.tier, node.os.cpu.percent, node.thread_pool.write.completed, node.thread_pool.write.rejected,
node.thread_pool.write.queue, node.thread_pool.write.active, node.thread_pool.write.largest,
node.thread_pool.write.threads, node.thread_pool.system_write.completed, node.thread_pool.system_write.rejected,
node.thread_pool.system_write.queue, node.thread_pool.force_merge.completed,
node.thread_pool.force_merge.rejected, node.thread_pool.force_merge.queue, node.thread_pool.force_merge.active,
node.thread_pool.merge.completed, node.thread_pool.merge.queue, node.thread_pool.merge.active
```

## Metric Guidance

- `node_count_and_ips`: Meaning: Total ingest-role nodes and unique IPs. Healthy pattern: Node count == unique IPs means no shared hardware. Investigation trigger: If unique IPs < node count, multiple nodes share an allocator/host (common in ECE), meaning they share CPU/memory resources.
- `nodes_by_tier`: Meaning: Breakdown of nodes with ingest-related roles (hot, ingest, warm, coord, data). Healthy pattern: Dedicated ingest nodes handle pipeline processing; hot nodes handle shard writes. Investigation trigger: Missing dedicated ingest nodes means hot nodes bear both pipeline and indexing load.
- `configuration_variations`: Meaning: Whether nodes in same tier have consistent CPU cores, heap, RAM, storage, ES version, OS version. Healthy pattern: All values should be 1 (identical config across tier). Green = consistent. Investigation trigger: Red with >1 means inconsistent hardware in a tier. Small RAM/storage differences (few MB) are normal in cloud. Large differences cause unbalanced performance. Check Nodes Configuration dashboard to validate.
- `load_average_15m`: Meaning: CPU utilization percent per node. Combines CPU, disk I/O, and network. Load of 1.0 = fully loaded single core. Load == core count means fully utilized. Healthy pattern: All nodes in same tier should have roughly equal load. Investigation trigger: Uneven load = workload imbalance or hotspotting. NOT container-aware: in Kubernetes/shared hosts, load reports host-level, making nodes appear overloaded. Only accurate on full-size dedicated VMs.
- `shard_avg_indexing_time_by_node`: Meaning: Indexing CPU time per shard normalized by JVM uptime. Only counts active write shards. Healthy pattern: Even distribution across hot nodes. Investigation trigger: High bars on few nodes = hotspotting. Trace to specific high-volume data streams (e.g., traces-apm, logs-k8s). 1 primary + 1 replica config concentrates load on 2 nodes. Consider increasing shard count for high-volume streams.
- `http_clients`: Meaning: Current open HTTP connections and total opened since restart, per node. Healthy pattern: Even balance across nodes in same tier = traffic routing is balanced. Investigation trigger: Uneven current_open suggests load balancer or proxy misconfiguration. Dedicated ingest nodes will have more connections than hot nodes (traffic routes to ingest first).
- `indexing_pressure`: Meaning: Memory used for indexing, split into coordinating, primary, and replica categories. Current = snapshot of right now. Total = cumulative since restart (more useful for balance analysis). Healthy pattern: Coordinating pressure balanced across receiving nodes (driven by traffic routing/proxy). Primary and replica pressure driven by shard allocation. Investigation trigger: One node with much lower coordinating = traffic routing imbalance or uneven uptime (recent restart). Imbalanced primary/replica = uneven shard allocation. Any rejections = queue overflow, serious issue.
- `workload_percent`: Meaning: Percentage of node CPU time spent on each activity (indexing, query, ingest pipeline, merge). Healthy pattern: Hot nodes ~90%% indexing is ideal. Ingest pipeline 10-20%% on hot nodes is normal. Dedicated ingest nodes show ~100%% ingest pipeline time. Investigation trigger: Ingest pipeline >20%% on hot nodes = heavy/complex pipelines, investigate grok/dissect errors. High query time = security detections, ML jobs, or high-cardinality aggregations consuming hot-node resources.
- `workload_duration`: Meaning: Total accumulated milliseconds spent per activity per node. Healthy pattern: Correlates with shard activity seen in indexing time chart. Investigation trigger: One node with dramatically different duration = imbalanced shard allocation or different uptime.
- `write_thread_pool`: Meaning: Thread pool for external write requests. Size = CPU core count. Healthy pattern: No rejections. Queue near zero. Completed count roughly balanced (unless shard allocation is uneven). Largest == size means all threads were used at once (acceptable if no queue buildup). Investigation trigger: Any rejections = queue full (10k default), serious throughput issue. Unbalanced completed count = trace to shard allocation or traffic routing (especially private link zone misalignment).
- `system_write_thread_pool`: Meaning: Capped at 5 threads, handles system index writes. Healthy pattern: Low activity, balanced on ingest nodes (internal traffic). Won't be balanced on hot nodes (system indices are sparse). Investigation trigger: High queue or unusual activity could indicate a bug (historical: ingest pipelines once used system_write instead of write).
- `force_merge_thread_pool`: Meaning: Force merge operations (compaction after rollover). Healthy pattern: No queue. Only on data nodes with shards (never on dedicated ingest). Best on hot tier (fast disks, spread across many nodes). Target rollover at 30GB so force merge takes 20-40min. Investigation trigger: ANY queue is a bad sign. Force merge on 50GB shard can take 2 hours. If force merge takes longer than rollover interval, queue builds indefinitely, eventually fills disk and causes write blocks. Since ES 9.2, force merge runs on clone (primary only), so some imbalance is normal.
- `merge_thread_pool`: Meaning: Regular (non-force) merge thread pool, new in ES 9.0/8.19. Auto-scaling, not fixed to CPU cores. Healthy pattern: No excessive numbers. Balanced correlates with shard activity. Investigation trigger: Extremely unbalanced = underlying shard hotspot (secondary signal, confirm with other charts).

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/ingest-nodes-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
