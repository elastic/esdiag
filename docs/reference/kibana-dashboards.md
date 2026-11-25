---
type: Reference
title: Kibana dashboards
description: Packaged dashboards, data views, and data streams for processed Kibana diagnostics and host SAR metrics.
tags: [kibana, dashboards, data-streams]
---

# Kibana dashboards

`esdiag setup output <saved-output>` installs these dashboards in the `esdiag`
Kibana space, together with the required Elasticsearch templates. They are
tagged `Kibana`, `ESDiag`, and `Beta`. The `Kibana Overviews` links panel at
the top of each dashboard keeps the current time range and filters when you
switch dashboards, so a `diagnostic.id` or `node.name` filter follows you.

Each Kibana diagnostic records a single point in time for one Kibana node.
Panels therefore compare nodes, or several diagnostics of the same node, rather
than plotting trends.

## Kibana Host Health

Data views: `Kibana Node Stats` (`metrics-kibana.node-esdiag*`) and
`Kibana Status` (`metrics-kibana.status-esdiag*`).

- Summary tiles for node count, the node with the highest heap use, event loop
  delay p99, and average response time.
- Node status. Kibana 8.x and later report `status.overall.level`. Kibana 7.x
  reports `status.overall.state`.
- Process memory, heap used of the heap limit, event loop delay and
  utilization, OS load, and OS memory per node.
- Response times, requests, disconnects, concurrent connections, and
  Elasticsearch client sockets per node.
- Container CPU quota, throttling, and cgroup memory. Only Kibana 8.x and
  later on Linux report these values. Other nodes show no data.
- A per-node table with every metric above.

## Kibana Task Manager

Data views: `Kibana Task Manager` (`metrics-kibana.task_manager-esdiag*`) and
`Kibana Task Manager Task Types`
(`metrics-kibana.task_manager.task_type-esdiag*`).

- Overall and per-section health, and configuration such as poll interval,
  claim strategy, and capacity.
- Workload tiles. Every Kibana node reports the same cluster-wide workload, so
  the tiles use the maximum and never sum across nodes.
- Drift, load, and polling percentiles, plus claim conflicts and mismatches per
  node.
- Capacity estimation from Kibana 7.16 and later. It compares observed Kibana
  instances with proposed instances, and maximum throughput with required
  throughput.
- The top 10 task types by failed percentage and by p99 drift, and a task type
  table sorted by p99 drift.

## Task type documents

Task Manager health reports per-task-type values keyed by task type name, and
those names contain dots. ESDiag writes one document per task type to
`metrics-kibana.task_manager.task_type-esdiag`, with the fields
`task_type.name`, `task_type.workload`, `task_type.drift`,
`task_type.duration`, and `task_type.result_frequency`. The summary document
in `metrics-kibana.task_manager-esdiag` omits those per-task-type maps.

Kibana diagnostics processed before these dashboards existed have no task
type documents or concurrent connection counts. Reprocess them to fill those
panels.

## Host Activity (SAR)

Data view: `Host Activity (SAR)` (`metrics-host.sar-esdiag`). Four dashboards,
modeled on [kSar](https://github.com/vlsi/ksar), chart this stream. They are
tagged `Elasticsearch`, `ESDiag`, and `Beta`. Each opens with the
`Elasticsearch` links panel on the left and the shared `Hosts Overviews` links
panel, which connects the four dashboards, on the right. Hosts Overview, linked
last from the `Elasticsearch` links, carries the same pair.

ESDiag fills this stream from `syscalls/sar.txt` in Elasticsearch bundles. Unlike
the Kibana dashboards above, SAR covers a time window, so the panels plot trends.
The metrics describe a *host*, and a host may run several Elasticsearch nodes,
for example on an ECE allocator or a Kubernetes node. Series therefore break
down by `host.name`, never `node.name`.

Every document has one `sar.metricset`:

| `sar.metricset` | One document per | Entity field |
|---|---|---|
| `summary` | sample: host-wide CPU, memory, swap, paging, load, sockets, kernel tables, TCP, and NFS | none |
| `cpu` | sample and CPU | `sar.cpu.id` |
| `io` | sample and block device | `sar.io.device` |
| `network` | sample and interface | `sar.network.interface` |
| `filesystem` | sample and filesystem | `sar.filesystem.name` |
| `interrupt`, `tty`, `fan`, `temp`, `in`, `usb`, `fc_host` | sample and entity | `sar.<metricset>.*` |

Filter on `sar.metricset` before aggregating. The `summary` documents hold the
host-wide `all` CPU row, and the `cpu` documents hold one row per core, both
under `sar.cpu.*`. Without the filter, an average mixes them. Every panel
on the dashboards applies this filter.

Each dashboard groups its charts into collapsible sections that follow kSar's
graphs, with one metric per chart:

- **Host Activity: Compute** opens with two charts across all cores: the average of
  each used component (`usr`, `nice`, `sys`, `iowait`, `steal`, `irq`, `soft`,
  `guest`, `gnice`), stacked, and the 95th percentile of busy
  (`1 - sar.cpu.idle.pct`) and of each component. A four-wide grid follows with
  one stacked chart per CPU, 0 through 127, in sections of 32; only the first
  is expanded. Collapsed sections chart frequency and softnet rates by
  `sar.cpu.id`, and interrupts by host and by `sar.interrupt.id`. Per-interrupt
  rates come from `sar -I` rows or, when sar printed one column per interrupt
  and CPU, from those per-CPU values summed across CPUs.
- **Host Activity: Storage** opens with the host-wide `sar -b` totals: total
  transfers per second beside tiles for the block device count, average and
  peak transfers per second, max queue depth (`aqu_sz`), max await, and peak
  utilization, then read, write, and discard transfers and blocks per second.
  Utilization and Bandwidth sections chart utilization, transfers, queue size,
  await, read, write, and discard bandwidth, and request size by
  `sar.io.device`. A collapsed section charts filesystem space and inode use
  by `sar.filesystem.name`.
- **Host Activity: Network** charts traffic and error rates by
  `sar.network.interface`.
- **Host Activity: System** charts load, processes, memory, swap, paging, huge
  pages, kernel tables, sockets, TCP, and NFS client and server activity, by
  `host.name`, so several hosts or diagnostics can be compared.

The Compute, Storage, and Network charts aggregate every host in the selection, so
filter to one `host.name` first. Kibana cannot require that filter.

Every dashboard has controls for `host.name` and `elasticsearch.node.name`.
Compute, Network, and System add `diagnostic.id`; Compute adds `sar.cpu.id`;
Storage adds `sar.io.device_class`; Network adds `sar.network.interface`. A
panel whose section is missing from `sar.txt` shows no data. Selecting a device
class also empties the Storage charts and tiles that have no device: the
aggregate transfers and the Filesystems section.

`sar` names block devices either by kernel name (`sda`, `dm-3`) or by number
(`dev8-0`, `dev253-3`), depending on its options. `sar.io.device_class` gives
both forms one class, so the Device Class control selects the same devices
either way:

| `sar.io.device_class` | Kernel names | Device major numbers |
|---|---|---|
| `sd` | `sda`, `sdb1` | 8, 65–71, 128–135 |
| `dm` | `dm-0` | 253 |
| `nvme` | `nvme0n1`, `nvme0n1p2` | 259 |
| `md` | `md0` | 9 |
| `vd` | `vda` | none |
| `xvd` | `xvda` | 202 |
| `sr` | `sr0` | 11 |
| `loop` | `loop0` | 7 |
| `other` | anything else | anything else |

Major 253 is assigned dynamically, so the `dm` class for `devM-N` names is a
best guess. Filesystem inode counts are `sar.filesystem.ifree.count` and
`sar.filesystem.iused.count`, beside the `sar.filesystem.iused.pct` ratio.

`sar` prints local wall-clock times without a timezone, so ESDiag stores them as
UTC and sets `sar.timezone` to `assumed_utc`. If the host ran in another
timezone, the samples are shifted by that offset.

`elasticsearch.node.id` and `elasticsearch.node.name` list the bundle's nodes
that run on the host. `elasticsearch.node.match` records how ESDiag found them:

| Value | Evidence |
|---|---|
| `listen_socket` | A TCP listening socket in `syscalls/ss.txt` or `syscalls/netstat.txt` matches a node's transport or HTTP publish address. Co-located nodes on one IP are told apart by port. |
| `k8s_node_name` | The SAR hostname equals a node's `k8s_node_name` attribute (ECK), or its short name does and every match names the same Kubernetes node. `kubernetes.node.name` is set too. |
| `hostname` | The SAR hostname equals a node's host, name, or HTTP publish host, or matches one of them unambiguously by short name. |
| `manifest_host` | The bundle manifest's collection target equals a node's host, IP, or HTTP publish host. |
| `none` | No evidence. `elasticsearch.node.id` and `elasticsearch.node.name` are omitted. |
