# Hosts Overview

## Purpose

Retrieves host-level metrics: node-to-host mapping with node counts per host, CPU load average (15m) and CPU utilization percent per node, CGroup CPU throttling (time throttled and throttle count), cluster topology summary (nodes, hosts, IPs, nodes-per-host ratio). Includes expert commentary for identifying host contention, CPU throttling in containerized environments, and co-location issues.

## Required Input

- `diagnostic_id`: a verified diagnostic ID, for example `cluster-name@YYYY-MM-DD~wxyz`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Replace `{{diagnostic_id}}` in the ES|QL examples with the verified diagnostic ID before execution.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Host Summary

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}" | STATS nodes_on_host = COUNT(*), total_cpu = SUM(node.os.available_processors)
BY node.host, node.tier
```

### Host Cpu Load

```esql
FROM metrics-node-esdiag* | WHERE diagnostic.id == "{{diagnostic_id}}" | KEEP node.name, node.host, node.tier, node.os.cpu.percent, node.os.available_processors,
node.os.cpu.percent, node.os.cgroup.cpu.stat.time_throttled_nanos, node.os.cgroup.cpu.stat.number_of_times_throttled
```

### Cluster Topology

```esql
FROM settings-node* | WHERE diagnostic.id == "{{diagnostic_id}}"
| STATS node_count = COUNT(*), unique_hosts = COUNT_DISTINCT(node.host), unique_ips
= COUNT_DISTINCT(node.ip) | EVAL nodes_per_host = ROUND(node_count * 1.0 / unique_hosts,
1)
```

## Metric Guidance

- `host_vs_node`: Multiple ES nodes can share a single host. Healthy: typically 1 node per host. Multiple nodes per host is common in large clusters to utilize high-core-count machines. If nodes_per_host > 1, verify each node has dedicated heap and CPU affinity. Resource contention between co-located nodes causes unpredictable latency.
- `load_average_15m`: 15-minute load average per node. Compare against allocated_processors (cores). Healthy: load_avg / cores < 1.0. Ratio > 1.5 sustained means host is overcommitted. In K8s, load reports host-level not container-level, so co-located pods inflate values.
- `cpu_throttling`: CGroup CPU throttling (time_throttled_nanos and number_of_times_throttled). Healthy: zero or near-zero. Any throttling means container CPU limit is too low. Throttled nodes show spiky latency. Increase CPU limits or reduce workload.
- `cpu_percent`: OS-level CPU utilization (0-100). Healthy: below 80%% sustained. Above 80%% means capacity is needed. Unlike load_average, this IS container-aware.
- `ip_vs_host`: Compare unique IPs vs hosts. IPs > hosts means multiple network interfaces. Hosts > IPs means shared IP (NAT/proxy), which can cause discovery issues.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/hosts-overview?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```
