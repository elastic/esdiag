# Lifecycle Overview

## Purpose

Retrieves Index Lifecycle Management (ILM) metrics: index counts per policy and phase, phase-to-node-tier alignment (detecting shards stuck on wrong tier), shard size distribution by phase (min/avg/median/max in GB), dataset size by age bucket and policy, ILM error counts, and force merge backlog (shards in forcemerge action by tier). Includes expert commentary for identifying ILM transition delays, oversharding, oversized shards, force merge bottlenecks, and retention policy health. Use when the user asks about ILM, index lifecycle, data retention, phase transitions, force merge queues, or shard sizing by lifecycle phase.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Indices By Policy Phase

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND index.ilm.policy IS NOT NULL | STATS index_count = COUNT(*), dataset_size
= SUM(index.total.store.total_data_set_size_in_bytes) BY index.ilm.policy, index.ilm.phase
```

### Phase Tier Alignment

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND index.ilm.phase IS NOT NULL | STATS shard_count = COUNT(*) BY index.ilm.phase,
node.tier
```

### Shard Size By Phase

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND index.ilm.phase IS NOT NULL | STATS shard_count = COUNT(*), min_size_gb
= ROUND(MIN(shard.store.size_in_bytes) / 1073741824.0, 2), avg_size_gb = ROUND(AVG(shard.store.size_in_bytes)
/ 1073741824.0, 2), max_size_gb = ROUND(MAX(shard.store.size_in_bytes) / 1073741824.0,
2), p50_size_gb = ROUND(MEDIAN(shard.store.size_in_bytes) / 1073741824.0, 2)
BY index.ilm.phase
```

### Size By Age Policy

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND index.ilm.policy IS NOT NULL | EVAL age_days = ROUND(index.age / 86400000.0),
age_bucket = CASE(age_days < 1, "0-1d", age_days < 7, "1-7d", age_days < 14,
"7-14d", age_days < 30, "14-30d", age_days < 90, "30-90d", age_days < 180, "90-180d",
"180d+") | STATS dataset_size = SUM(index.total.store.total_data_set_size_in_bytes),
index_count = COUNT(*) BY age_bucket, index.ilm.policy
```

### Ilm Status

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND index.ilm.policy IS NOT NULL | STATS total_managed = COUNT(*), in_error
= SUM(CASE(index.ilm.step == "ERROR", 1, 0)), no_ilm = SUM(CASE(index.ilm.managed
== false, 1, 0)) | LIMIT 1
```

### Force Merge Backlog

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND index.ilm.action == "forcemerge" | STATS shard_count = COUNT(*), avg_size_gb
= ROUND(AVG(shard.store.size_in_bytes) / 1073741824.0, 2) BY node.tier, index.ilm.step
```

## Metric Guidance

- `ilm_policies_and_indices`: Meaning: Which ILM policies exist and how many indices each manages. Common policies include metrics, logs, APM, alerts, profiling, plus custom policies. Healthy pattern: All data stream indices should be ILM-managed. Investigation trigger: Indices without ILM have no automatic lifecycle management and need manual cleanup.
- `indices_by_phase`: Meaning: Index count per ILM phase (hot/warm/cold/frozen/delete). Healthy pattern: Hot phase has actively writing indices. Frozen has historical data. Investigation trigger: Many indices stuck in one phase may indicate ILM delays or misconfiguration. Frozen having 4x the shards of hot is normal for long-retention clusters.
- `index_count_by_age`: Meaning: How many indices were created per day (driven by rollover frequency). Healthy pattern: Consistent daily pattern with weekly/monthly spikes from max_age rollovers (7d or 30d common). Investigation trigger: Sudden changes in rollover pattern = new data sources onboarded or ILM policy changes. Weekend dips are normal for business-domain data.
- `dataset_size_by_age`: Meaning: Total data volume per age bucket, colored by ILM policy. Healthy pattern: Logs typically dominate volume. Metrics compress better (especially with time_series mode). Investigation trigger: Sudden dips in volume = possible ingestion issues or workload changes. Data dropping off sharply at 30/90 days = retention policies working correctly. Old data still present beyond expected retention = ILM may be falling behind.
- `phase_to_node_alignment`: Meaning: Whether shards in each ILM phase are on the correct node tier. Healthy pattern: Hot phase shards on hot nodes, warm on warm, frozen on frozen. Investigation trigger: Shards in frozen phase still on warm/hot nodes = ILM transition delayed or backlogged. Common cause: force merge queue on warm nodes is too deep, blocking transitions. Warm nodes have fewer CPUs than hot, can only force merge one shard at a time, creating bottleneck.
- `shard_size_by_phase`: Meaning: Shard size distribution per ILM phase (boxplot-style min/avg/max). Healthy pattern: 20-40GB average shard size in warm/frozen. 10-60GB acceptable range. Investigation trigger: Mean <3GB = oversharding, will cause shard count problems at scale. Shards >50GB in frozen = may have been caught during force merge (can temporarily report 2x actual size). Shards >100GB that show complete status = genuinely oversized, investigate rollover settings.
- `shard_size_distribution`: Meaning: Shard count by size bucket and tier. Repeats from Data Summary but contextualized by ILM phase. Healthy pattern: Majority of shards in 10-60GB range. Investigation trigger: Many megabyte-sized shards = rolling over too frequently or low-volume data streams with aggressive rollover. Many 60-100GB+ shards = not rolling over soon enough, risk slow recovery and OOM on searches.
- `ilm_errors`: Meaning: Count of indices in ILM error state. Healthy pattern: Zero errors. Investigation trigger: Any errors need immediate investigation. Common causes: disk watermark reached, snapshot repository unavailable, insufficient permissions, or node allocation filters preventing shard movement.
- `force_merge_backlog`: Meaning: Shards stuck in force_merge ILM step. Healthy pattern: Minimal concurrent force merges. Investigation trigger: >10 shards all force merging simultaneously = excessive backlog. Force merge on warm nodes is problematic because fewer nodes with lower CPU means deeper queues. Best practice: force merge on hot tier (more nodes, faster disks, better parallelism). Target 30GB rollover size so force merge completes in 20-40 minutes. If force merge takes longer than rollover interval, queue builds indefinitely leading to disk exhaustion.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/lifecycle-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
