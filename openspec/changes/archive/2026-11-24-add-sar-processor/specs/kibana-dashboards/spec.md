## ADDED Requirements

### Requirement: Host Activity Dashboards
The system SHALL provide four Host Activity dashboards over `metrics-host.sar-esdiag`: **Compute**, **Storage**, **Network**, and **System**. It SHALL also provide one data view over the stream, so that every ingested field is explorable in Discover and Lens even without a dashboard panel.

The dashboards follow kSar's page structure. Each kSar graph is one collapsible dashboard section; where a dashboard holds a single kSar graph, each of its plots is a section. Every chart plots exactly one metric. Every panel SHALL filter on `sar.metricset`, so that summary and entity documents are never aggregated together.

SAR metrics describe a host. The dashboards expect the user to filter to one host first, because Kibana controls cannot require a selection. Host-wide charts break series down by `host.name`, never `node.name`, so that several SAR inputs can be compared.

Every dashboard SHALL open with two links panels, by reference to shared links objects: the Elasticsearch overview links on the left and the "Hosts Overviews" links, linking all four dashboards, on the right. Hosts Overview SHALL carry the same pair, with the Hosts Overviews links in place of the Nodes Overviews links, and the Elasticsearch overview links SHALL link to Hosts Overview last, after Search.

Every dashboard SHALL provide controls for `host.name` and `elasticsearch.node.name`, plus its own controls:
- Compute: `diagnostic.id` and `sar.cpu.id`.
- Storage: `sar.io.device_class`.
- Network: `diagnostic.id` and `sar.network.interface`.
- System: `diagnostic.id`.

#### Scenario: Cross-linked dashboards
- **GIVEN** the shipped Kibana saved objects
- **WHEN** the user opens any Host Activity dashboard or Hosts Overview
- **THEN** the Elasticsearch overview links panel is on the left
- **AND** the Hosts Overviews links panel, linking to the Compute, Storage, Network, and System dashboards, is on the right
- **AND** the links keep the current time range

#### Scenario: Hosts Overview navigation
- **GIVEN** the shipped Kibana saved objects
- **WHEN** the user opens any dashboard with the Elasticsearch overview links
- **THEN** its last link, after Search, opens Hosts Overview
- **AND** Hosts Overview shows the Elasticsearch overview and Hosts Overviews links panels, not the Nodes Overviews links

#### Scenario: Host and co-located nodes
- **GIVEN** SAR documents for a host correlated with two Elasticsearch nodes
- **WHEN** the user opens the System dashboard
- **THEN** the host appears once per `host.name` series, not once per node
- **AND** filtering `elasticsearch.node.name` to either node shows that host's data

#### Scenario: All CPUs
- **GIVEN** `cpu` documents for a host with several CPUs
- **WHEN** the user opens the Compute dashboard
- **THEN** its first section shows two full-width charts computed across all cores in each time bucket:
  - **Averages**: the average of each used CPU component (`usr`, `nice`, `sys`, `iowait`, `steal`, `irq`, `soft`, `guest`, `gnice`), stacked, so the stack height is the average busy ratio.
  - **p95**: the 95th percentile of the busy ratio (`1 - sar.cpu.idle.pct`) and of each used component, unstacked.
- **AND** neither chart includes the `summary` documents' `all` row

#### Scenario: Per-CPU grid
- **GIVEN** the Compute dashboard
- **WHEN** the user expands a per-CPU section
- **THEN** the dashboard shows one chart per CPU ID, four charts wide, for CPU IDs 0 through 127 in four sections of 32 (`CPUs 0-31` through `CPUs 96-127`)
- **AND** each chart stacks the used CPU components for its CPU, like kSar
- **AND** only the `CPUs 0-31` section is expanded by default, and charts for CPU IDs the host lacks show no data rather than an error

#### Scenario: Compute sections
- **GIVEN** the Compute dashboard
- **THEN** collapsed sections show, broken down by `sar.cpu.id`: CPU frequency (`mhz`, `wghmhz`) and CPU network softnet (`total`, `dropd`, `squeezd`, `rx_rps`, and `flw_lim` per second)
- **AND** an Interrupts section shows total interrupts per second by `host.name` and per-interrupt rates by `sar.interrupt.id`

#### Scenario: Storage
- **GIVEN** `io` documents
- **WHEN** the user opens the Storage dashboard
- **THEN** an expanded "I/O (all devices)" section opens with aggregate block I/O (`sar.block.*`) by `host.name`: total transfers per second beside summary tiles for the block device count, average and peak transfers per second, max `aqu_sz` queue depth, max `await_ms`, and peak `util.pct`, followed by read, write, and discard transfers and blocks per second
- **AND** expanded sections for the kSar block-device plots show, one metric per chart broken down by `sar.io.device`:
  - Utilization: `util.pct`, `tps`, `aqu_sz`, and `await_ms`.
  - Bandwidth: read, write, and discard `bytes_per_sec`, and `areq.bytes`.
- **AND** the `sar.io.device_class` control narrows every per-device chart and tile to one class, such as `sd` or `dm`; the aggregate `sar.block` charts and tiles and the Filesystems charts carry no device class, so they show no data while a class is selected
- **AND** a collapsed Filesystems section shows filesystem used, free, and inodes used by `sar.filesystem.name`

#### Scenario: Network
- **GIVEN** `network` documents
- **WHEN** the user opens the Network dashboard
- **THEN** an Interfaces traffic section shows rx and tx packets, rx and tx bytes, rx and tx compressed, rx multicast, and `ifutil.pct`
- **AND** an Interfaces errors section shows rx and tx errors, rx and tx drops, rx frame and FIFO errors, tx carrier and FIFO errors, and collisions
- **AND** every chart plots one metric broken down by `sar.network.interface`

#### Scenario: System
- **GIVEN** `summary` documents
- **WHEN** the user opens the System dashboard
- **THEN** it has one section per kSar graph, each chart one metric broken down by `host.name`: Load, Processes, KMem, KSwap, Swap, Paging, HugePages, Kernel VFS, Sockets, TCP, NFS Client, and NFS Server

#### Scenario: Panels without data
- **GIVEN** a host whose `sar.txt` lacks a section, such as NFS or filesystems
- **WHEN** the user opens a dashboard charting it
- **THEN** those charts show no data rather than an error

#### Scenario: Data view names the owned stream
- **GIVEN** the shipped Kibana saved objects
- **WHEN** the data view naming contract test runs
- **THEN** the SAR data view MUST name exactly `metrics-host.sar-esdiag`
