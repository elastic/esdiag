# Index Operations Overview

## Purpose

Retrieves per-index and per-node operation statistics: total indexing time/count, query time/count, fetch time/count, get time/count aggregated across all shards; top 15 indices ranked by indexing time with query metrics; and operation time breakdown by node for balance detection. Includes expert commentary for indexing hotspots, expensive query identification, fetch overhead analysis, and node imbalance diagnosis. Use when the user asks about index operations, indexing performance, query performance, search operations, fetch overhead, get operations, or per-index workload.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Operation Totals

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS total_indexing_time_ms = SUM(shard.indexing.index_time_in_millis), total_indexing_count
= SUM(shard.indexing.index_total), total_query_time_ms = SUM(shard.search.query_time_in_millis),
total_query_count = SUM(shard.search.query_total), total_fetch_time_ms = SUM(shard.search.fetch_time_in_millis),
total_fetch_count = SUM(shard.search.fetch_total), total_get_time_ms = SUM(shard.get.time_in_millis),
total_get_count = SUM(shard.get.total)
```

### Top Indices By Indexing

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS indexing_time = SUM(shard.indexing.index_time_in_millis), indexing_count
= SUM(shard.indexing.index_total), query_time = SUM(shard.search.query_time_in_millis),
query_count = SUM(shard.search.query_total) BY index.name | SORT indexing_time
DESC | LIMIT 15
```

### Operations By Node

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS indexing_time = SUM(shard.indexing.index_time_in_millis), query_time
= SUM(shard.search.query_time_in_millis), fetch_time = SUM(shard.search.fetch_time_in_millis),
get_time = SUM(shard.get.time_in_millis) BY node.name
```

## Metric Guidance

- `indexing_time`: Meaning: Total CPU time spent on indexing operations across all shards. Measured in milliseconds. This reflects how much work the cluster is doing to ingest documents. Healthy pattern: Evenly distributed across data nodes. Investigation trigger: If indexing time is concentrated on a few indices, those are hotspots consuming disproportionate resources. Compare indexing_time across nodes to check for imbalance. A single node with 2-3x the indexing time of peers indicates uneven shard allocation or routing. High indexing_time with low indexing_count means each document is expensive to index (heavy mappings, ingest pipelines, or large documents).
- `query_time`: Meaning: Total CPU time spent executing search queries across all shards. High values indicate either expensive queries (aggregations, wildcards, regex) or very high query volume. Healthy pattern: Query time should be proportional to query count. Investigation trigger: High query_time with low query_count = expensive individual queries. Check for unoptimized queries, missing keyword fields, or heavy aggregations. High query_time on specific indices means those indices are search hotspots. Compare across nodes; uneven query_time suggests search traffic is not balanced.
- `fetch_time`: Meaning: Time spent fetching actual documents after the query phase identifies matches. The query phase finds matching doc IDs; the fetch phase retrieves the _source and highlighted fields. Healthy pattern: fetch_time should be a small fraction of query_time (typically 5-15%%). Investigation trigger: fetch_time approaching or exceeding query_time indicates large result sets (high size parameter), heavy _source filtering, or excessive highlighting. High fetch_count relative to query_count means queries are returning many hits. Reduce fetch overhead by using source filtering, smaller page sizes, or scroll/search_after.
- `get_time`: Meaning: Time spent on GET-by-ID operations (direct document retrieval by _id). Healthy pattern: Usually very low or zero unless the application relies on point lookups. Investigation trigger: High get_time indicates the application is doing many direct document fetches rather than search queries. This is common in key-value style usage patterns. If unexpected, check for update-heavy application flows (GET before UPDATE) or application-level caching issues. Get operations bypass the query cache, so high get volume does not benefit from caching.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/index-operations-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
