# Daily Indexing Overview

## Purpose

Retrieves daily indexing volume metrics for capacity planning: total estimated indexing bytes/day (on-disk after compression, primaries only - use for retention sizing), total bulk bytes/day (raw uncompressed JSON through Bulk API), top 15 data streams by daily volume with store configurations, top 10 rollover aliases by volume, and compression effectiveness breakdown by store config (mode/source/codec). Includes expert commentary for capacity planning, compression ratio analysis, and storage optimization opportunities. Use when the user asks about daily ingest volume, indexing rates, bulk rates, retention planning, compression ratios, or storage efficiency.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Daily Totals

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}" | STATS total_est_indexing_bytes_day = SUM(CASE(index.is_write_index == true, index.primaries.indexing.est_bytes_per_day, NULL)), total_est_bulk_bytes_day = SUM(CASE(index.is_write_index == true, index.primaries.bulk.est_bytes_per_day, NULL)), total_indices = COUNT(index.name), data_streams = COUNT_DISTINCT(index.data_stream.name), rollover_aliases = COUNT(index.alias.name)
```

### Top Datastreams Indexing

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}" AND index.data_stream.name IS NOT NULL | STATS est_indexing = SUM(index.primaries.indexing.est_bytes_per_day), est_bulk = SUM(index.primaries.bulk.est_bytes_per_day), backing_indices = COUNT(*) BY index.data_stream.name, index.store.config | SORT est_indexing DESC | LIMIT 15
```

### Top Rollover Aliases

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}" AND index.lifecycle.rollover_alias IS NOT NULL | STATS est_indexing = SUM(index.primaries.indexing.est_bytes_per_day), est_bulk = SUM(index.primaries.bulk.est_bytes_per_day), index_count = COUNT(*) BY index.lifecycle.rollover_alias, index.store.config | SORT est_indexing DESC | LIMIT 10
```

### Rates By Store Config

```esql
FROM metrics-index-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}" | STATS est_indexing = SUM(index.primaries.indexing.est_bytes_per_day), est_bulk = SUM(index.primaries.bulk.est_bytes_per_day), index_count = COUNT(*) BY index.store.config
```

## Metric Guidance

- `daily_indexing_calculation`: Meaning: Estimated bytes stored per day on primary shards only, excluding replicas. Calculated from index creation date to first ILM rollover action date, then averaged over that period. Only includes data streams and rollover aliases; standalone indices are excluded because they never rollover, so there is no stop point for measurement. Healthy pattern: Use for retention planning. Example: 1TB/day indexing * 30 days retention = 30TB minimum storage needed. Hot tier needs ~3x daily rate for data, replicas, headroom for watermarks, and merges.
- `indexing_rate_vs_bulk_rate`: Meaning: Indexing rate = on-disk size after compression (primaries only). Bulk rate = raw uncompressed JSON data through Bulk API including metadata and enrichment. Healthy pattern: Bulk rate significantly higher than indexing rate means good compression. 18:1 or 20:1 ratios seen with logsdb mode. Lower ratios (2:1 to 5:1) with standard/default. Investigation trigger: If indexing rate is close to bulk rate, compression is ineffective; check store config. Storage config does not impact bulk rate, only indexing rate changes when you improve compression.
- `store_config_impact`: Meaning: Color coding in charts shows which storage config (standard/logsdb/time_series) each data stream uses. logsdb gets very high compression ratios. time_series compresses metrics even better. standard/default is the least efficient. standard/synthetic/best_compression avoids logsdb merge time increase while still getting good compression. Investigation trigger: Large data streams on standard/default/default = biggest optimization opportunity. Switch to best_compression or logsdb to reduce indexing rate without affecting bulk rate.
- `top_data_streams`: Meaning: Top data streams by estimated daily indexing volume. Healthy pattern: Top 5 data streams typically account for >50%% of all volume. Focus optimization efforts here for maximum impact. Investigation trigger: Many small data streams with high total count = fragmented ingest, lower max throughput.
- `data_stream_count`: Meaning: Total active data streams. Each has at least one actively writing shard. Healthy pattern: Fewer data streams = higher max throughput due to write economies of scale. Investigation trigger: >500 data streams is high. Many tiny data streams fragment ingest and reduce throughput. Consolidate namespaces where possible.
- `rollover_aliases`: Meaning: Legacy index management pattern. Most clusters have migrated to data streams. Healthy pattern: Small fraction of total data. Investigation trigger: Large volumes in rollover aliases could benefit from migration to data streams for better management.
- `compression_ratio`: Meaning: Ratio of bulk_rate to indexing_rate indicates overall compression effectiveness. Healthy pattern: logsdb achieves 10:1 to 20:1. time_series even higher for metrics. best_compression alone gives 3:1 to 5:1. Investigation trigger: Low ratio (<3:1) means standard/default config - significant savings available. After changing store config, bulk rate stays same but indexing rate drops = more efficient = goal.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/daily-indexing-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
