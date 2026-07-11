# Nodes Configuration Overview

## Purpose

Retrieves node hardware and configuration details: full node list with name, tier, IP, host, version, CPU cores, OS version, heap size, and availability zone; node counts by tier/role; and configuration variation analysis per tier (CPU, heap, version, OS consistency). Includes expert commentary for config consistency checks, heap sizing guidance, AZ distribution review, version alignment, and shared-hardware detection. Use when the user asks about node configuration, hardware specs, node roles, tier breakdown, heap sizing, availability zones, or version consistency.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Node Configuration

```esql
FROM settings-node* | WHERE diagnostic.id == "{{diagnostic_id}}"
| KEEP node.name, node.tier, node.ip, node.host, node.version, node.os.allocated_processors,
node.os.version, node.jvm.mem.heap_max_in_bytes, node.attributes.availability_zone
```

### Nodes By Role

```esql
FROM settings-node* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS node_count = COUNT(*) BY node.tier
```

### Config Variations By Tier

```esql
FROM settings-node* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS cpu_configs = COUNT_DISTINCT(node.os.allocated_processors), heap_configs
= COUNT_DISTINCT(node.jvm.mem.heap_max_in_bytes), version_configs = COUNT_DISTINCT(node.version),
os_configs = COUNT_DISTINCT(node.os.version) BY node.tier
```

## Metric Guidance

- `config_consistency`: All nodes in the same tier should have identical CPU, heap, RAM, storage, and version. Green = consistent (count_distinct == 1). Red = mismatch (count_distinct > 1). Inconsistent hardware in a tier causes unbalanced performance because Elasticsearch distributes shards evenly regardless of node capacity. The weakest node becomes the bottleneck.
- `heap_sizing`: Hot nodes typically need 30GB heap for optimal indexing throughput. Master nodes need heap proportional to shard and index count, roughly 1GB per 30k shards. Too-small heap causes frequent GC pauses and potential OOM. Too-large heap wastes memory that could be used for OS filesystem cache, which accelerates search and merge operations.
- `availability_zones`: Nodes should be spread across availability zones for high availability. All nodes in one AZ is a single point of failure. Ideally each tier has nodes in at least 2 AZs (3 for production). Uneven AZ distribution causes unbalanced replica allocation.
- `version_consistency`: Mixed versions are only acceptable during rolling upgrades. In steady state all nodes should run the same version. Mixed versions can cause subtle compatibility issues and prevent certain features from being enabled cluster-wide.
- `ip_vs_node_count`: If unique IPs is less than node count, multiple nodes share the same underlying hardware (common in ECE/ECK deployments). Shared hardware means shared CPU and memory resources, so nodes on the same host compete for resources under load.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/nodes-configuration-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
