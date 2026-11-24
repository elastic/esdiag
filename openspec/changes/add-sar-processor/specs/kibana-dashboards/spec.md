## ADDED Requirements

### Requirement: Host Activity (SAR) Dashboard
The system SHALL provide a Host Activity (SAR) dashboard and one data view over `metrics-host.sar-esdiag`. The data view makes every ingested field explorable in Discover and Lens, even without a dashboard panel.

Every panel SHALL filter on `sar.metricset`, so that summary and entity documents are never aggregated together. The dashboard SHALL break host series down by `host.name`, not `node.name`. It SHALL provide controls for:
- `diagnostic.id`
- `host.name`
- `elasticsearch.node.name`
- `sar.cpu.id`
- `sar.io.device`
- `sar.network.interface`
- `sar.filesystem.name`

#### Scenario: Host and co-located nodes
- **GIVEN** SAR documents for a host correlated with two Elasticsearch nodes
- **WHEN** the user opens the Host Activity (SAR) dashboard
- **THEN** the host appears once per `host.name` series, not once per node
- **AND** filtering `elasticsearch.node.name` to either node shows that host's data

#### Scenario: CPU, load, and memory panels
- **GIVEN** `summary` documents
- **WHEN** the user views the host panels
- **THEN** the dashboard shows:
  - CPU `usr`, `sys`, `iowait`, `steal`, and `idle` ratios over time.
  - `ldavg_1`, `ldavg_5`, and `ldavg_15` with `host.cpu.count`.
  - `runq_sz` and `blocked`.
  - Memory used ratio, available, cached, dirty, and slab bytes.
  - Swap used bytes.
  - `majflt_per_sec`, `pgsteal_per_sec`, `pswpin_per_sec`, and `pswpout_per_sec`.

#### Scenario: Per-core CPU panel
- **GIVEN** `cpu` documents for a host with several CPUs
- **WHEN** the user views the per-core panel
- **THEN** the dashboard shows busy ratio (`1 - sar.cpu.idle.pct`) over time broken down by `sar.cpu.id`, so that a single saturated core is visible behind a moderate host-wide average
- **AND** the panel's values MUST NOT include the `summary` documents' `all` row

#### Scenario: Disk saturation panels
- **GIVEN** `io` documents
- **WHEN** the user views the disk panels
- **THEN** the dashboard shows `await_ms`, `util.pct`, `aqu_sz`, `tps`, and read and write `bytes_per_sec` broken down by `sar.io.device`

#### Scenario: Filesystem panel
- **GIVEN** `filesystem` documents
- **WHEN** the user views the filesystem panel
- **THEN** the dashboard shows `fsused.pct` and `iused.pct` over time broken down by `sar.filesystem.name`

#### Scenario: Network panels
- **GIVEN** `network` documents
- **WHEN** the user views the network panels
- **THEN** the dashboard shows rx and tx bytes, packets, drops, and errors per second broken down by `sar.network.interface`
- **AND** it shows TCP `active_per_sec`, `passive_per_sec`, and ETCP `retrans_per_sec` from `summary` documents

#### Scenario: Sockets and kernel tables
- **GIVEN** `summary` documents
- **WHEN** the user views the kernel panels
- **THEN** the dashboard shows `totsck`, `tcpsck`, `tcp_tw`, `file_nr`, `inode_nr`, and `cswch_per_sec` over time

#### Scenario: Panels without data
- **GIVEN** a host whose `sar.txt` has no filesystem section
- **WHEN** the user opens the dashboard
- **THEN** the filesystem panel shows no data rather than an error

#### Scenario: Data view names the owned stream
- **GIVEN** the shipped Kibana saved objects
- **WHEN** the data view naming contract test runs
- **THEN** the SAR data view MUST name exactly `metrics-host.sar-esdiag`
