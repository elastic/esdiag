# Index Settings Overview

## Purpose

Retrieves index configuration and storage efficiency metrics: cluster version and license, index counts (total/data streams/rollover aliases/standalone), store configuration distribution (index mode/source/codec with dataset sizes), refresh interval settings, primary shard and replica configurations, top 15 data streams by size with namespace counts, and data volume by membership type. Includes expert commentary for identifying compression opportunities, over-sharding, refresh interval tuning, and data organization best practices. Use when the user asks about index settings, compression, storage efficiency, data stream organization, shard/replica configuration, or refresh intervals.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Cluster Version License

```esql
FROM metrics-diagnostic-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| KEEP cluster.version.number, diagnostic.license.type | LIMIT 1
```

### Index Counts

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS total_indices = COUNT(*), data_streams = COUNT_DISTINCT(index.data_stream.name),
backing_indices = SUM(CASE(index.data_stream.name IS NOT NULL, 1, 0)), rollover_aliases
= COUNT_DISTINCT(index.lifecycle.rollover_alias), rollover_indices = SUM(CASE(index.lifecycle.rollover_alias
IS NOT NULL, 1, 0)), standalone_indices = SUM(CASE(index.data_stream.name IS
NULL AND index.lifecycle.rollover_alias IS NULL, 1, 0))
```

### Store Config Distribution

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS dataset_size = SUM(index.total.store.total_data_set_size_in_bytes),
index_count = COUNT(*) BY index.mode, index.source, index.codec
```

### Refresh Intervals

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS index_count = COUNT(*) BY index.refresh_interval
```

### Shards Replicas

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS index_count = COUNT(*), total_dataset = SUM(index.total.store.total_data_set_size_in_bytes)
BY index.number_of_shards, index.number_of_replicas
```

### Top Datastreams

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND index.data_stream.name IS NOT NULL | STATS dataset_size = SUM(index.total.store.total_data_set_size_in_bytes),
backing_count = COUNT(*), namespaces = COUNT_DISTINCT(index.data_stream.namespace)
BY index.data_stream.type, index.data_stream.dataset | SORT dataset_size DESC
| LIMIT 15
```

### Size By Membership

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| EVAL membership = CASE(index.data_stream.name IS NOT NULL, "datastream", index.lifecycle.rollover_alias
IS NOT NULL, "rollover_alias", "standalone") | STATS dataset_size = SUM(index.total.store.total_data_set_size_in_bytes),
index_count = COUNT(*) BY membership
```

## Metric Guidance

- `store_config_distribution`: Meaning: Data distribution by index mode (standard/logsdb/time_series), source (default/synthetic/stored), and codec (default/best_compression). The combination of these three determines compression ratio. Healthy pattern: Majority should use best_compression or logsdb/time_series modes, which imply best_compression. standard/default/default is the least efficient storage. Investigation trigger: Large amounts in standard/default/default means significant compression savings are available. Best compression is almost always worth the trade-off. Savings in disk I/O and smaller segments can improve throughput. logsdb/time_series modes use index sorting, which increases merge time; recommend 30GB max shard size instead of 50GB.
- `license_and_version`: Meaning: Cluster version and license level. License determines synthetic source eligibility. Healthy pattern: Enterprise license enables synthetic source for best compression. Investigation trigger: Platinum or below falls back to stored source in logsdb/time_series modes, reducing compression ratio meaningfully.
- `index_membership`: Meaning: How indices are organized - data streams, rollover aliases, or standalone. Healthy pattern: Majority of data in data streams (modern best practice). Investigation trigger: Large standalone indices miss ILM/rollover benefits. Many rollover aliases may indicate legacy config that could migrate to data streams.
- `data_size_by_membership`: Meaning: Total data volume split by data stream vs rollover alias vs standalone. Healthy pattern: Most volume in data streams. Investigation trigger: Significant standalone data = no automatic lifecycle management.
- `refresh_interval`: Meaning: How often indices become searchable after writes. Default is 1 second. Healthy pattern: 5-30 seconds for ingest workloads. Serverless defaults to 5s. -1 means disabled. Investigation trigger: Many indices at 1s default = suboptimal ingest throughput. Refresh is not free (blocks index, requires disk I/O). 10-30s common for high volume. Diminishing returns above 30s as refresh work becomes too large per cycle.
- `indexing_complete`: Meaning: Whether index is still being written to (relevant for data streams/rollover). Read-only indices are stable and won't change. Healthy pattern: Rolled-over indices should be marked indexing_complete. Investigation trigger: Active write indices that should have rolled over but haven't.
- `primary_shards_and_replicas`: Meaning: Heatmap of shard/replica configurations across indices. Healthy pattern: 1 primary shard (default) with 1 replica for HA. Only need more replicas for extreme query throughput. Investigation trigger: >1 replica is unusual for observability/security - adds ingest load and storage with little benefit. Exception: Enrich indices use auto_expand_replicas (0-all) to have one copy per node for pipeline efficiency. System indices use auto_expand_replicas (0-1). Many primary shards on small datasets = over-sharding (check if dataset justifies the shard count, target ~50GB per shard).
- `datastream_size_distribution`: Meaning: Data volume by data stream (type/dataset/namespace convention). Healthy pattern: Top 5 data streams are majority of data. Fewer namespaces = better economies of scale. Investigation trigger: Too many namespaces fragments write throughput and increases data stream count. Consolidate namespaces where possible for efficiency.
- `rollover_alias_distribution`: Meaning: Data volume by rollover alias (legacy pattern). Healthy pattern: Small fraction of total data. Investigation trigger: Large rollover alias data could benefit from migration to data streams.
- `index_detail_table`: Meaning: Per-index details including store config, shards, replicas, dataset size, ILM policy, age, data stream membership. Use for drill-down after identifying issues in summary panels. Investigation trigger: Large standalone indices without ILM policy. Indices with many shards but small dataset size. Old indices still in hot tier (check ILM policy assignment).

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/index-settings-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
