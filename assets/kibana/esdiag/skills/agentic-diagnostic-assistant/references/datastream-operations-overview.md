# Datastream Operations Overview

## Purpose

Retrieves per-datastream operation statistics: total indexing/query/fetch/get time and document counts across all data stream shards, top 15 data streams ranked by indexing time with query and fetch time breakdowns, and operation time distribution by node for data stream shards. Use when the user asks about data stream performance, indexing hotspots, query distribution across nodes, fetch-vs-query ratios, or per-datastream operation breakdowns.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Datastream Totals

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND index.data_stream.name IS NOT NULL | STATS total_indexing_time = SUM(shard.indexing.index_time_in_millis),
total_indexing_count = SUM(shard.indexing.index_total), total_query_time = SUM(shard.search.query_time_in_millis),
total_query_count = SUM(shard.search.query_total), total_fetch_time = SUM(shard.search.fetch_time_in_millis),
total_fetch_count = SUM(shard.search.fetch_total), total_get_time = SUM(shard.get.time_in_millis),
total_get_count = SUM(shard.get.total)
```

### Top Datastreams By Activity

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND index.data_stream.name IS NOT NULL | STATS indexing_time = SUM(shard.indexing.index_time_in_millis),
query_time = SUM(shard.search.query_time_in_millis), fetch_time = SUM(shard.search.fetch_time_in_millis)
BY index.data_stream.name | SORT indexing_time DESC | LIMIT 15
```

### Datastream Ops By Node

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND index.data_stream.name IS NOT NULL | STATS indexing_time = SUM(shard.indexing.index_time_in_millis),
query_time = SUM(shard.search.query_time_in_millis), fetch_time = SUM(shard.search.fetch_time_in_millis)
BY node.name
```

## Metric Guidance

- `datastream_indexing`: Meaning: Active write shards in data streams drive indexing load. indexing.index_time_in_millis measures cumulative time spent in Lucene indexing operations across all shards for each data stream. indexing.index_total is the total number of documents indexed. Healthy pattern: Top 5 data streams typically dominate 80%%+ of all indexing time. Focus optimization on the highest-time streams first for maximum impact. Investigation trigger: High indexing time with low doc count = complex mappings, heavy ingest pipelines, or large documents.
- `query_distribution_by_node`: Meaning: query_time_in_millis and query_total per node for data stream shards only. Uneven query time across nodes indicates shard allocation imbalance or search hotspot. Healthy pattern: Query time should be roughly proportional across nodes holding similar shard counts. Investigation trigger: One node with 3x the query time of peers = search hotspot. Check shard allocation, routing preferences, and whether hot-tier nodes are overloaded.
- `fetch_vs_query`: Meaning: Fetch phase retrieves actual document content after query phase identifies matching docs. High fetch time relative to query time = large result sets being retrieved, large _source fields, or expensive highlighting. Healthy pattern: Fetch time should be a small fraction (10-20%%) of query time for well-tuned searches. Investigation trigger: Fetch time > 50%% of query time = reduce result size (smaller pages), use source filtering, or add _source exclusions to reduce payload.
- `get_operations`: Meaning: GET by ID operations (realtime gets). Usually from ingest enrichment or application lookups. Healthy pattern: Low get time and count relative to indexing and search. Investigation trigger: High get time = enrich processor bottleneck or application doing single-doc lookups instead of bulk search.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/datastream-operations-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
