# Search Summary

## Purpose

Comprehensive search activity statistics. Returns total search metrics (query time/count, fetch time/count, scroll time/count, get time/count), thread pool totals (search completed/rejected/queue, search_worker completed/rejected, esql completed/rejected, search_coordination completed/rejected), and per-node search breakdown (query time, query count, fetch time, rejections by pool). Includes expert guidance on query vs fetch balance, scroll usage patterns, get operation overhead, thread pool rejection thresholds, and ES|QL adoption. Use when the user asks about search performance summary, search statistics, search thread pool health, search rejections, or overall search activity.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Search Totals

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS total_query_time = SUM(node.indices.search.query_time_in_millis), total_query_count
= SUM(node.indices.search.query_total), total_fetch_time = SUM(node.indices.search.fetch_time_in_millis),
total_fetch_count = SUM(node.indices.search.fetch_total), total_scroll_time
= SUM(node.indices.search.scroll_time_in_millis), total_scroll_count = SUM(node.indices.search.scroll_total),
total_get_time = SUM(node.indices.get.time_in_millis), total_get_count = SUM(node.indices.get.total)
```

### Search Thread Pool Totals

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS search_completed = SUM(node.thread_pool.search.completed), search_rejected
= SUM(node.thread_pool.search.rejected), search_queue = SUM(node.thread_pool.search.queue),
search_worker_completed = SUM(node.thread_pool.search_worker.completed), search_worker_rejected
= SUM(node.thread_pool.search_worker.rejected), esql_completed = SUM(node.thread_pool.esql.completed),
esql_rejected = SUM(node.thread_pool.esql.rejected), search_coord_completed
= SUM(node.thread_pool.search_coordination.completed), search_coord_rejected
= SUM(node.thread_pool.search_coordination.rejected)
```

### Search By Node

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| KEEP node.name, node.tier, node.indices.search.query_time_in_millis, node.indices.search.query_total,
node.indices.search.fetch_time_in_millis, node.thread_pool.search.rejected,
node.thread_pool.search_worker.rejected, node.thread_pool.esql.rejected
```

## Metric Guidance

- `query_vs_fetch`: Query phase finds matching docs, fetch phase retrieves them. High fetch time relative to query = large result sets. Reduce page size or use scroll.
- `scroll_usage`: High scroll count/time = heavy export operations or legacy search patterns. Prefer search_after or PIT for pagination.
- `get_operations`: GET by ID operations. Usually low. High values = app doing many point lookups instead of bulk/multi-get.
- `thread_pool_rejections`: ANY rejections in search/search_worker/esql pools = capacity issue. Need more nodes or reduce query load.
- `esql_activity`: ES|QL has dedicated thread pools. Growing usage shows adoption of the new query language.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/search-summary?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
