# Node Tasks Overview

## Purpose

Retrieves node task metrics including persistent and transport tasks: task counts with total and average running time by action type; task distribution across nodes for balance analysis; and per-node CPU load average versus available processors for overload detection. Includes expert commentary for task action interpretation, distribution balance, load-to-core ratio analysis, and long-running task identification. Use when the user asks about node tasks, task distribution, persistent tasks, transport tasks, node load, CPU load average, task balance, or overloaded nodes.

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
2) BY task.action | SORT total_time_ms DESC | LIMIT 20
```

### Tasks By Node

```esql
FROM metrics-task-* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS task_count = COUNT(*), total_time_ms = ROUND(SUM(task.running_time_in_nanos)
/ 1000000.0, 0) BY node.name
```

### Node Load

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| KEEP node.name, node.tier, `node.os.cpu.load_average.1m`, node.os.available_processors
```

## Metric Guidance

- `task_actions`: cluster:monitor tasks are normal housekeeping. indices:data/write/bulk are indexing tasks and their volume reflects write load. Long-running persistent tasks (like ML jobs, transforms, watches) are expected and should not be treated as stuck. transport:* actions are internal cluster communication.
- `task_distribution`: Task counts should be roughly even across nodes of the same tier. If one node has significantly more tasks or higher total time than peers of the same role, investigate routing imbalance, uneven shard allocation, or that node being slower due to hardware issues or GC pressure. Coordinating-only nodes will naturally have fewer shard-level tasks. Load average vs cores: divide node.os.cpu.load_average.1m by node.os.available_processors. A ratio above 1.0 means the node is overloaded and tasks will queue. A ratio below 0.7 is comfortable. Between 0.7 and 1.0 is busy but manageable. Sustained high load on a single node while others are idle points to hot-spotting.
- `long_tasks`: Tasks running longer than 5 minutes need investigation unless they are known persistent tasks such as ML jobs, transforms, or watches. Check if long-running tasks are concentrated on a single node, which would indicate that node is a bottleneck. Compare task duration with node load to correlate overload with slow tasks.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/node-tasks-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
