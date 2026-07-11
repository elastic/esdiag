# Ingest Summary

## Purpose

Retrieves a summary of ingest pipeline and indexing activity: overall CPU time breakdown (total indexing time, query time, fetch time, ingest pipeline time), top 10 pipelines ranked by total processing time (time_in_millis, count), top 10 processors ranked by time with failure counts (type, time, count, failed), and top 10 pipelines ranked by failure count. Includes expert commentary for identifying ingest bottlenecks, CPU-intensive processor types (grok, dissect, script), failure patterns, and pipeline consolidation opportunities. Use when the user asks about ingest summary, pipeline performance overview, ingest vs indexing time, processor failures, or pipeline bottlenecks.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Ingest Cpu Time

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS total_indexing_time = SUM(node.indices.indexing.index_time_in_millis),
total_query_time = SUM(node.indices.search.query_time_in_millis), total_fetch_time
= SUM(node.indices.search.fetch_time_in_millis), total_ingest_time = SUM(node.ingest.total.time_in_millis)
```

### Top Pipelines

```esql
FROM metrics-ingest.pipeline-* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS pipeline_time = SUM(ingest.pipeline.time_in_millis), pipeline_count
= SUM(ingest.pipeline.count) BY ingest.pipeline.name | SORT pipeline_time DESC
| LIMIT 10
```

### Top Processors

```esql
FROM metrics-ingest.processor-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS proc_time = SUM(ingest.processor.time_in_millis), proc_count = SUM(ingest.processor.count),
proc_failed = SUM(ingest.processor.failed) BY ingest.processor.type | SORT proc_time
DESC | LIMIT 10
```

### Pipeline Failures

```esql
FROM metrics-ingest.pipeline-* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS pipeline_time = SUM(ingest.pipeline.time_in_millis), pipeline_failed
= SUM(ingest.pipeline.failed) BY ingest.pipeline.name | SORT pipeline_failed
DESC | LIMIT 10
```

## Metric Guidance

- `ingest_vs_indexing`: Meaning: Compare total ingest pipeline time to total indexing time. If ingest time is >20%% of indexing time, pipelines may be a bottleneck. Healthy pattern: Ingest pipeline time should be a small fraction of total indexing time on hot/data nodes. Investigation trigger: If ingest time approaches or exceeds indexing time, pipelines are the bottleneck. Scale ingest nodes or simplify heavy processors.
- `processor_types`: Meaning: Different processor types have very different CPU costs. grok and dissect processors are CPU-intensive due to regex/pattern matching. script processors can be slow if poorly written or if they invoke expensive operations. set, remove, rename processors are cheap metadata operations. Healthy pattern: Top processors by time should be expected heavy processors (grok, dissect) with zero failures. Investigation trigger: If script or grok processors dominate time, review patterns for optimization.
- `failures`: Meaning: Pipeline and processor failures waste resources because documents are parsed, fail, and may retry. Common causes: grok pattern mismatches on unexpected log formats, script errors, dissect failures. Healthy pattern: Zero failures across all pipelines and processors. Investigation trigger: Any non-zero failure count needs investigation. Fix grok patterns, add conditionals to skip non-matching docs, or use on_failure handlers.
- `pipeline_consolidation`: Meaning: Many small pipelines add overhead due to per-pipeline coordination costs. Each pipeline invocation has fixed overhead for setup, teardown, and context switching. Healthy pattern: Fewer pipelines with higher throughput each. Investigation trigger: If there are many pipelines with very low counts but non-trivial time, consider consolidating them.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/ingest-summary?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
