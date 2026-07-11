# Elasticsearch Cluster Report

## Purpose

Retrieves high-level Elasticsearch cluster health metrics: total nodes, indices, shards (with budget targets), doc/indexing rates, local and total storage, CPU load and disk utilization by tier, and average shards per node by tier. Includes expert commentary for anomaly detection. Use when the user asks about overall cluster health, node counts, capacity planning, or tier-level resource usage.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Cluster Totals

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS total_nodes = COUNT(*), total_docs = SUM(node.indices.docs.count), local_store_bytes
= SUM(node.indices.store.size_in_bytes), total_dataset_bytes = SUM(node.indices.store.total_data_set_size_in_bytes)
```

### Load Disk By Tier

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS min_cpu = MIN(node.os.cpu.percent), avg_cpu = AVG(node.os.cpu.percent),
max_cpu = MAX(node.os.cpu.percent), min_disk = MIN(node.fs.total.used_percent),
avg_disk = AVG(node.fs.total.used_percent), max_disk = MAX(node.fs.total.used_percent),
avg_shards = AVG(node.indices.shard_stats.total_count), node_count = COUNT(*)
BY node.tier
```

### Index Shard Counts

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS total_indices = COUNT(*), est_bulk_bytes_day = SUM(index.primaries.bulk.est_bytes_per_day),
est_indexing_bytes_day = SUM(index.primaries.indexing.est_bytes_per_day)
```

### Shard Totals

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS total_shards = COUNT(*), primary_shards = SUM(CASE(shard.routing.primary
== true, 1, 0))
```

## Metric Guidance

- `total_nodes`: Meaning: Total nodes in the cluster. Healthy pattern: Ideally no more than 100-150 nodes per cluster to maintain stability and keep cluster state size manageable. Investigation trigger: Above 150 nodes expect slower cluster state updates, delayed responses, reduced stability.
- `total_indices`: Meaning: Total indices in the cluster. Target ~3000 indices per GB of master node heap. Healthy pattern: With 30GB master heap, budget is ~90k indices (45-60k is comfortable range). Investigation trigger: Exceeding budget strains master node and cluster state distribution.
- `total_shards`: Meaning: Total shard count (primary+replica). Budget target ~100k shards max. Healthy pattern: Below 100k operates predictably. Since v8, ES handles higher counts better but APIs like shard-level stats become burdensome above 100k. Investigation trigger: Above 100k risk OOM on stats APIs, slow recovery, rough edges in certain APIs. doc_rate (node.indices.indexing.index_total summed): Meaning: Total document indexing operations across all nodes. Healthy pattern: Stable rate matching expected ingest volume. Investigation trigger: Sudden drops suggest ingestion failures; spikes suggest new data sources. index_rate (index.total.indexing.est_bytes_per_day summed): Meaning: Estimated bytes indexed per day including replicas. Healthy pattern: Proportional to doc rate with expected document sizes. Use primary-only rate for long-term retention sizing. Investigation trigger: Rate close to bulk rate means poor compression.
- `total_docs`: Meaning: Total document count across all local nodes. Healthy pattern: Target ~200M docs per shard. Investigation trigger: If total_docs/total_shards >> 200M, shards may be too large.
- `total_dataset`: Meaning: Total addressable data across all tiers including frozen/searchable snapshots (not just local disk). Investigation trigger: Compare against local_store to understand how much data lives in searchable snapshots vs local storage.
- `local_store`: Meaning: Data stored locally on data nodes. Does NOT include frozen/searchable snapshot data. Healthy pattern: With 1 replica, total local should be ~2x primary store. Investigation trigger: Ratio other than 2:1 suggests missing or extra replicas.
- `load_percent_by_tier`: Meaning: CPU utilization percent distribution per tier. NOT container-aware - reports host-level load. Healthy pattern: Hot nodes moderate load; master nodes may appear overloaded if small (<60GB memory) due to resource sharing on host. Investigation trigger: In Kubernetes/containers, nodes sharing hosts will look overloaded even when host is fine. Frozen nodes at high load is normal. Only trust this metric on full-size dedicated VMs (e.g., Elastic Cloud nodes >60GB).
- `disk_utilization_by_tier`: Meaning: Disk usage percentage per tier. Accurate per-node metric. Healthy pattern: Frozen tier at 95-98%% is NORMAL - it pre-reserves 90%% of disk for shared cache. ML and master nodes have near-zero disk usage (no data path needed). Investigation trigger: Hot/warm nodes above 85%% need attention. Frozen in red is expected and OK. Data path and OS often on different mounts so high frozen usage does not risk OS disk.
- `avg_shards_per_node_by_tier`: Meaning: Average shard count per node grouped by tier role. Ingest nodes = hot tier (receiving new data). Search nodes = warm/cold (search-focused). Store nodes = frozen (storage-optimized). Healthy pattern: Fewer shards per hot node = better ingest throughput. Lowest latency search clusters have ~1 shard/node; high-density observability clusters have hundreds per node (optimizing storage over latency). Investigation trigger: Hot tier always appears high due to data stream naming conventions and integrations creating many indices. Consider excluding system indices from count for a clearer picture.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/elasticsearch-cluster-report?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
