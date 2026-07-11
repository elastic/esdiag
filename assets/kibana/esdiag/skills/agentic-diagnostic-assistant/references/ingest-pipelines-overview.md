# Ingest Pipelines Overview

## Purpose

Retrieves ingest pipeline and processor statistics: top 15 pipelines ranked by total processing time (time_in_millis, count), top 15 processors ranked by time with failure counts (type, name, time, count, failed), pipeline processing time distribution across nodes, and pipeline time versus indexing time comparison per node. Includes expert commentary for identifying pipeline bottlenecks, processor failures, and node imbalances. Use when the user asks about ingest pipeline performance, processor failures, pipeline bottlenecks, or ingest time distribution.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Top Pipelines

```esql
FROM metrics-ingest.pipeline-* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS total_time = SUM(ingest.pipeline.time_in_millis), total_count = SUM(ingest.pipeline.count)
BY ingest.pipeline.name | SORT total_time DESC | LIMIT 15
```

### Top Processors

```esql
FROM metrics-ingest.processor-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS total_time = SUM(ingest.processor.time_in_millis), total_count = SUM(ingest.processor.count),
total_failed = SUM(ingest.processor.failed) BY ingest.processor.type, ingest.processor.name
| SORT total_time DESC | LIMIT 15
```

### Pipeline Time By Node

```esql
FROM metrics-ingest.pipeline-* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS pipeline_time = SUM(ingest.pipeline.time_in_millis) BY node.name
```

### Pipeline Vs Indexing

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS indexing_time = SUM(node.indices.indexing.index_time_in_millis), total_ingest_time
= SUM(node.ingest.total.time_in_millis) BY node.name
```

## Metric Guidance

- `pipeline_time`: Meaning: Total CPU time spent in ingest pipelines. Top pipelines by time show where processing cost is concentrated. Healthy pattern: Pipeline time should be less than 10-20%% of total node workload on hot nodes. Investigation trigger: If a single pipeline dominates total time, inspect its processors for heavy operations such as grok, script, or enrich.
- `processor_failures`: Meaning: Any failures in ingest processors need investigation. Common causes: grok pattern mismatches, script errors, dissect failures. Healthy pattern: Zero failures across all processors. Investigation trigger: Failures cause retries and waste resources. Check the specific processor type and pipeline name to identify the failing integration or data stream.
- `pipeline_balance_across_nodes`: Meaning: Distribution of pipeline processing time across nodes. Healthy pattern: Should be even if traffic routing is balanced across ingest/hot nodes. Investigation trigger: Uneven distribution indicates a proxy or load balancer issue, or that some nodes have recently restarted (lower cumulative time).
- `pipeline_vs_indexing_time`: Meaning: Comparison of total ingest pipeline time versus total indexing time per node. Healthy pattern: Pipeline time should be a small fraction of indexing time. Investigation trigger: If pipeline time is close to or exceeds indexing time, pipelines are the bottleneck. Optimize heavy processors: simplify grok patterns, remove unnecessary processors, consider ingest node scaling.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/ingest-pipelines-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
