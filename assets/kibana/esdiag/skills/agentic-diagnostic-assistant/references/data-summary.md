# Data Summary

## Purpose

Retrieves comprehensive data summary for an Elasticsearch cluster diagnostic. Returns store sizes, dataset sizes by codec/mode/tier/age, shard counts and distribution, document counts, data stream counts, and indexing rates. 15 dashboard panels consolidated into 5 ES|QL queries. Use when the user asks about cluster storage, data distribution, shard sizing, or indexing rates.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Cluster Data Summary

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS total_store = SUM(index.total.store.size_in_bytes), primary_store =
SUM(index.primaries.store.size_in_bytes), total_dataset = SUM(index.total.store.total_data_set_size_in_bytes),
total_docs = SUM(index.total.docs.count), primary_docs = SUM(index.primaries.docs.count),
total_shards = SUM(index.total.shard_stats.total_count), primary_shards = SUM(index.primaries.shard_stats.total_count),
data_streams = COUNT_DISTINCT(index.data_stream.name), index_count = COUNT(index.name),
est_indexing_bytes_day = SUM(index.primaries.indexing.est_bytes_per_day), est_bulk_bytes_day
= SUM(index.primaries.bulk.est_bytes_per_day)
```

### Dataset Size By Mode

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS dataset_size = SUM(index.total.store.total_data_set_size_in_bytes) BY
index.codec, index.mode
```

### Primary Shard Count

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND shard.routing.primary == true | STATS primary_shard_count = COUNT(*)
```

### Dataset Size By Tier Age

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| EVAL age_days = DATE_DIFF("day", index.creation_date, NOW()), age_bucket =
CASE(age_days < 1, "0-1d", age_days < 7, "1-7d", age_days < 30, "7-30d", age_days
< 90, "30-90d", age_days < 180, "90-180d", age_days < 365, "180d-1y", "1y+")
| STATS dataset_size = SUM(shard.store.total_data_set_size_in_bytes) BY node.tier,
age_bucket
```

### Shard Size Distribution

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| EVAL size_gb = shard.store.size_in_bytes / 1073741824.0, size_bucket = CASE(size_gb
< 1, "0-1GB", size_gb < 5, "1-5GB", size_gb < 10, "5-10GB", size_gb < 20, "10-20GB",
size_gb < 50, "20-50GB", "50GB+") | STATS shard_count = COUNT(*) BY node.tier,
size_bucket
```

## Metric Guidance

- `est_bulk_bytes_day`: Meaning: Uncompressed data volume through Bulk API per day, includes JSON expansion and shipper metadata. Healthy: Stable day-over-day, significantly larger than indexing rate. Alert: Sudden spikes=new data sources, drops=ingestion failures.
- `est_indexing_bytes_day`: Meaning: On-disk storage growth per day after compression, excludes Lucene merge I/O. Healthy: Proportional to bulk rate with compression savings. Alert: If close to bulk rate, compression may be ineffective.
- `total_store`: Meaning: Data on local data nodes only, excludes frozen/searchable snapshots. Healthy: Primary ~half of total (1 primary+1 replica). Alert: Ratio != 2:1 suggests missing or extra replicas.
- `total_dataset`: Meaning: Total addressable data across all tiers including frozen. Alert: Compare vs total_store to gauge searchable snapshot usage.
- `index_count`: Meaning: Total indices. Alert: Very high counts strain cluster state and master node.
- `total_docs`: Meaning: Total documents, target ~200M per shard. Alert: total_docs/total_shards >> 200M means shards too large.
- `total_shards/primary_shards`: Meaning: Shard count, budget ~100k max. Healthy: With searchable snapshots ratio != 2:1 (cold/frozen have no replicas). Alert: Approaching 100k is a concern.
- `data_streams`: Meaning: Active data streams, each has actively writing primary shard. Healthy: Fewer=higher max throughput. Alert: >500 fragments write throughput, <50 is ideal.
- `dataset_by_mode`: Meaning: Distribution across index modes and codecs. Healthy: Majority in best_compression or logsdb. Time series=highest compression for metrics. Alert: Large standard/default=missed compression savings.
- `dataset_by_tier_age`: Meaning: Data by tier and age. Healthy: Hot drops off in 7-8 days, frozen holds bulk of history. Alert: Old data in hot=ILM not working.
- `shard_distribution`: Meaning: Shard count by size bucket. Ideal 10-50GB. Healthy: Most shards 10-50GB, small <1GB ok in hot. Alert: Many <1GB in frozen=premature rollover, >50GB=OOM risk, slow recovery.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/data-summary?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
