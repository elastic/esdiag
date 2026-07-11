# Bulk Tasks Overview

## Purpose

Retrieves bulk and transport task metrics: task counts with total/average/max running time by action type; task count and total time by node for balance detection; and individual long-running tasks exceeding 60 seconds. Includes expert commentary for task distribution analysis, stuck operation identification, node imbalance diagnosis, and bulk indexing bottleneck interpretation. Use when the user asks about bulk tasks, running tasks, task distribution, long-running operations, bulk indexing performance, or node task balance.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Tasks By Action

```esql
FROM metrics-task-* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS task_count = COUNT(*), total_time_ms = ROUND(SUM(task.running_time_in_nanos)
/ 1000000.0, 0), avg_time_ms = ROUND(AVG(task.running_time_in_nanos) / 1000000.0,
2), max_time_ms = ROUND(MAX(task.running_time_in_nanos) / 1000000.0, 2) BY task.action
| SORT total_time_ms DESC | LIMIT 20
```

### Task Count By Node

```esql
FROM metrics-task-* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS task_count = COUNT(*), total_time_ms = ROUND(SUM(task.running_time_in_nanos)
/ 1000000.0, 0) BY node.name | SORT total_time_ms DESC
```

### Long Running Tasks

```esql
FROM metrics-task-* | WHERE diagnostic.id == "{{diagnostic_id}}"
AND task.running_time_in_nanos > 60000000000 | EVAL running_sec = ROUND(task.running_time_in_nanos
/ 1000000000.0, 1) | KEEP task.action, task.id, node.name, running_sec | SORT
running_sec DESC | LIMIT 20
```

## Metric Guidance

- `task_actions`: Bulk indexing tasks (indices:data/write/bulk) are the most common in a write-heavy cluster. cluster:monitor tasks are normal housekeeping performed by the master and coordinating nodes. transport actions like internal:coordination and cluster:admin are expected cluster overhead. A high count of indices:data/write/bulk[s] (the shard-level sub-action) means bulk requests are being distributed across many shards.
- `long_running_tasks`: Tasks running longer than 60 seconds may indicate slow bulk operations, resource contention on the target node, or stuck operations waiting on thread pool capacity. Investigate nodes with high task counts or long-running tasks for disk I/O saturation, high GC pressure, or thread pool rejections. A single long-running bulk task on one node while others complete quickly suggests that node is under-resourced or overloaded.
- `task_balance`: An even distribution of task counts and total time across nodes indicates healthy request routing and balanced shard allocation. If one node has significantly more tasks or higher total time than peers, check for: (1) routing imbalance where that node hosts more primary shards for hot indices, (2) the node being slower due to hardware issues, (3) a coordinating node funneling all bulk requests to itself.
- `bulk_task_time`: High average bulk task time suggests disk I/O bottleneck, heavy ingest pipelines adding processing overhead, indexing pressure causing throttling, or large bulk request sizes. Compare avg_time_ms across actions: if bulk actions are slow but monitor actions are fast, the bottleneck is in the write path. If all actions are slow, the node itself is overloaded.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/bulk-tasks-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
