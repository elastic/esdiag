# Shards Size Overview

## Purpose

Retrieves shard sizing and budget metrics: total/primary shard counts, small (<1GB) and large (>50GB) shard counts, shard distribution by ILM phase, shard size distribution by bucket and phase, and notable oversized/undersized shards with segment counts. Use when the user asks about shard sizes, budget, oversharding, or distribution.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Shard Budget

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}" | STATS total_shards = COUNT(*), primary_shards = SUM(CASE(shard.routing.primary == true, 1, 0)), small_shards_under_1gb = SUM(CASE(shard.store.total_data_set_size_in_bytes < 1073741824, 1, 0)), large_shards_over_60gb = SUM(CASE(shard.store.total_data_set_size_in_bytes > 64424509440, 1, 0))
```

### Shards By Phase

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}" | STATS shard_count = COUNT(*), primary_count = SUM(CASE(shard.routing.primary == true, 1, 0)) BY index.ilm.phase
```

### Size Distribution By Phase

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}" | EVAL size_bucket = CASE(shard.store.total_data_set_size_in_bytes < 1048576, "0-1MB", shard.store.total_data_set_size_in_bytes < 1073741824, "1MB-1GB", shard.store.total_data_set_size_in_bytes < 10737418240, "1-10GB", shard.store.total_data_set_size_in_bytes < 32212254720, "10-30GB", shard.store.total_data_set_size_in_bytes < 64424509440, "30-60GB", shard.store.total_data_set_size_in_bytes < 107374182400, "60-100GB", "100GB+") | STATS shard_count = COUNT(*) BY size_bucket, index.ilm.phase
```

### Notable Shards

```esql
FROM metrics-shard-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}" AND (shard.store.total_data_set_size_in_bytes > 64424509440 OR (index.ilm.phase == "frozen" AND shard.store.total_data_set_size_in_bytes < 1073741824)) | EVAL size_gb = ROUND(shard.store.total_data_set_size_in_bytes / 1073741824.0, 2) | KEEP index.name, index.data_stream.name, index.ilm.phase, node.tier, shard.routing.primary, shard.segments.count, shard.docs.count, size_gb | SORT size_gb DESC | LIMIT 20
```

## Metric Guidance

- `primary_shard_budget`: Meaning: Primary shard count determines total shard load (primary + replicas). Budget: <50k primary shards ideal. 50-75k acceptable. >75k with 1 replica = 150k+ total shards, approaching danger zone. Healthy pattern: Total shards <100k. Frozen/cold tiers often have no replica, so primary count there does not double. Investigation trigger: Project shard growth using daily creation rate * max retention days. Example: 1000 shards/day * 365 days = 365k shards - major red flag. If already at max retention and shard count is stable, small shards are acceptable though not ideal.
- `shard_size_distribution`: Meaning: Shard count by size bucket (<1MB, 1MB-1GB, 1-10GB, 10-30GB, 30-60GB, 60-100GB, 100GB+). Healthy pattern: Majority of shards in 10-60GB range. Investigation trigger: Many shards <1GB = oversharding problem. These shards rolled over too early or are low-volume data streams. Small shards in HOT tier are OK (haven't rolled over yet, still growing). Small shards in FROZEN tier are a problem - they already reached their final size and are permanently undersized. Many shards >60GB = not rolling over soon enough, risk slow recovery and OOM during searches.
- `small_shards_count`: Meaning: Total shards under 1GB. Healthy pattern: Should be a small fraction of total shards. Mostly in hot tier (still actively writing). Investigation trigger: If majority of ALL shards are <1GB, the cluster is headed for oversharding. Fix: Increase rollover max_age or max_primary_shard_size thresholds. Consolidate low-volume data streams.
- `large_shards_count`: Meaning: Total shards over 60GB. Healthy pattern: Zero or very few. Investigation trigger: Shards >60GB with multiple segments may be under active force merge (temporarily 2x actual size). Check shard.segments.count: >1 segment in frozen = likely still force merging. 1 segment in frozen and still >60GB = genuinely oversized. Check index mode/codec (switch to logsdb/best_compression). Reduce rollover max_primary_shard_size to 30GB for logsdb/time_series modes.
- `shards_by_ilm_phase`: Meaning: Shard distribution across ILM phases (hot/warm/cold/frozen). Healthy pattern: Hot phase has active write shards. Frozen has bulk of historical shards. Investigation trigger: Large number of warm shards = possible force merge bottleneck preventing transition to frozen. Hot shards being small is normal (still growing). Frozen shards being small is not (final size).
- `shard_detail_indicators`: Meaning: Per-shard details including segment count, doc count, and ILM phase. Key insight: segments.count == 1 means force merge is complete (expected in frozen). segments.count > 1 in frozen = still force merging or force merge failed. docs.count target: ~200M max per shard (automatic rollover trigger). Investigation trigger: Very high doc count with small shard size = tiny documents, consider batching or aggregating.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/shards-size-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
