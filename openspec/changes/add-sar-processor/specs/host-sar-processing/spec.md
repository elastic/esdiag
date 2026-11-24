## Purpose

Index the `sar -A` host activity history (`syscalls/sar.txt`) that ships in Elasticsearch diagnostic bundles as host-scoped time-series documents, correlated to every Elasticsearch node that runs on that host.

## ADDED Requirements

### Requirement: SAR source
SAR SHALL be the `sar` entry of the syscall source catalog: processing option `syscalls.sar`, file `syscalls/sar.txt`, implemented for Elasticsearch, and selected by default for the `standard` and `support` diagnostic types. Selection, missing-file handling, failure reporting, and platform independence SHALL follow the `syscall-processing` capability. When selected and present, the file SHALL be processed as `sar -A` text output. The system SHALL NOT read or decode binary sysstat data files (`saDD`, `sarDD`), even when the bundle contains them.

#### Scenario: Self-managed bundle with sar.txt
- **GIVEN** an Elasticsearch diagnostic bundle containing `syscalls/sar.txt` with valid `sar -A` output, processed with type `standard`
- **WHEN** the diagnostic is processed
- **THEN** host SAR documents MUST be exported to the `metrics-host.sar-esdiag` data stream

#### Scenario: Binary sysstat files are ignored
- **GIVEN** an Elasticsearch diagnostic bundle containing binary sysstat data files but no `syscalls/sar.txt`, processed with type `standard`
- **WHEN** the diagnostic is processed
- **THEN** no binary file MUST be read, no host SAR documents MUST be exported, and the SAR source MUST be reported as missing with no warning

### Requirement: Complete section coverage
The system SHALL ingest every sample row of every section that `sar -A` can emit. That covers the sections in this table, including per-core rows and every entity row, with nothing filtered as idle:

| Section (distinctive columns) | Field prefix | `sar.metricset` |
|---|---|---|
| CPU utilization (`CPU` with `%usr` or `%user`) | `sar.cpu.` | `all` row → `summary`; numbered rows → `cpu` |
| Softnet (`CPU` with `total/s`) | `sar.softnet.` | `all` row → `summary`; numbered rows → `cpu` |
| CPU clock (`CPU` with `MHz`) | `sar.cpu.` | `all` row → `summary`; numbered rows → `cpu` |
| Weighted CPU frequency (`CPU` with `wghMHz`) | `sar.cpu.` | `all` row → `summary`; numbered rows → `cpu` |
| Task creation and switching (`proc/s`, `cswch/s`) | `sar.task.` | `summary` |
| Swap activity (`pswpin/s`) and swap utilization (`kbswpfree`) | `sar.swap.` | `summary` |
| Paging (`pgpgin/s`) | `sar.paging.` | `summary` |
| Aggregate block I/O (`tps` with `bread/s`) | `sar.block.` | `summary` |
| Memory (`kbmemfree`) | `sar.memory.` | `summary` |
| Hugepages (`kbhugfree`) | `sar.hugepages.` | `summary` |
| Kernel tables (`dentunusd`) | `sar.kernel.` | `summary` |
| Load and run queue (`runq-sz`) | `sar.load.` | `summary` |
| Pressure stall (`%scpu-10`, `%sio-10`, `%smem-10`) | `sar.pressure.` | `summary` |
| Interrupt total (`INTR` row `sum`) | `sar.interrupts.` | `summary` |
| Sockets (`totsck`) and IPv6 sockets (`tcp6sck`) | `sar.sockets.` | `summary` |
| IP, EIP, ICMP, EICMP, TCP, ETCP, UDP (`irec/s`, `ihdrerr/s`, `imsg/s`, `ierr/s`, `active/s`, `atmptf/s`, `idgm/s`) | `sar.ip.`, `sar.eip.`, `sar.icmp.`, `sar.eicmp.`, `sar.tcp.`, `sar.etcp.`, `sar.udp.` | `summary` |
| IP6, EIP6, ICMP6, EICMP6, UDP6 (`irec6/s`, `ihdrer6/s`, `imsg6/s`, `ierr6/s`, `idgm6/s`) | `sar.ip6.`, `sar.eip6.`, `sar.icmp6.`, `sar.eicmp6.`, `sar.udp6.` | `summary` |
| NFS client (`call/s`) | `sar.nfs.` | `summary` |
| NFS server (`scall/s`) | `sar.nfsd.` | `summary` |
| Block devices (`DEV`) | `sar.io.` | `io` |
| Network traffic (`IFACE` with `rxpck/s`) and errors (`IFACE` with `rxerr/s`) | `sar.network.` | `network` |
| Filesystems (`FILESYSTEM` or `MOUNTPOINT`) | `sar.filesystem.` | `filesystem` |
| Per-interrupt rows (`INTR` other than `sum`, or one `N/s` column per interrupt) | `sar.interrupt.` | `interrupt` |
| Serial lines (`TTY`) | `sar.tty.` | `tty` |
| Fans (`FAN`), temperature (`TEMP`), voltage inputs (`IN`) | `sar.fan.`, `sar.temp.`, `sar.in.` | `fan`, `temp`, `in` |
| USB devices (`BUS` with `idvendor`) | `sar.usb.` | `usb` |
| Fibre Channel hosts (`FCHOST`) | `sar.fc_host.` | `fc_host` |

Within a recognized section, every column SHALL be exported. That includes columns added by sysstat versions not listed in this change: those use the generic naming rules under "Unit normalization".

When a timestamped header matches no section in this table, the system SHALL skip its rows and record one warning event per distinct header, naming its columns, so that unrecognized data is visible in the diagnostic report rather than silently dropped.

#### Scenario: Per-core CPU rows
- **GIVEN** a CPU utilization section with rows for `all` and CPUs `0` to `3` across 144 timestamps
- **WHEN** the file is processed
- **THEN** each `summary` document MUST carry the `all` values under `sar.cpu.*`, and 576 `cpu` documents MUST be exported, each with one `sar.cpu.id` from `0` to `3`

#### Scenario: Per-core sections merge per CPU
- **GIVEN** CPU utilization, softnet, and CPU clock sections that each report CPU `2` at the same timestamp
- **WHEN** the file is processed
- **THEN** exactly one `cpu` document MUST exist for CPU `2` at that timestamp, carrying `sar.cpu.usr.pct`, `sar.softnet.total_per_sec`, and `sar.cpu.mhz`

#### Scenario: NFS and protocol counters
- **GIVEN** NFS client, NFS server, TCP, and ETCP sections reporting a sample at the same timestamp
- **WHEN** the file is processed
- **THEN** the `summary` document for that timestamp MUST carry `sar.nfs.retrans_per_sec`, `sar.nfsd.scall_per_sec`, `sar.tcp.active_per_sec`, and `sar.etcp.retrans_per_sec` as distinct fields

#### Scenario: Idle entities are kept
- **GIVEN** a block device and the `lo` interface reporting `0.00` in every column for every sample
- **WHEN** the file is processed
- **THEN** their documents MUST still be exported

#### Scenario: Column from a newer sysstat
- **GIVEN** a `DEV` section containing an unlisted column `zq-cnt/s`
- **WHEN** the section is parsed
- **THEN** `io` documents MUST carry `sar.io.zq_cnt_per_sec` with the column's value

#### Scenario: Unrecognized section
- **GIVEN** a timestamped header whose columns match no known section, followed by data rows
- **WHEN** the file is processed
- **THEN** no document MUST contain values from those rows, the following known sections MUST parse normally, and the diagnostic report MUST record one warning naming the header's columns

### Requirement: Single host data stream
The system SHALL export every SAR document to one data stream, `metrics-host.sar-esdiag` (dataset `host.sar`), with one ESDiag index template. Every document SHALL carry `@timestamp` and a keyword `sar.metricset` that identifies its kind:
- `summary`: exactly one per host and sample timestamp. It merges every host-wide section and the `all` rows of per-CPU sections.
- Entity documents, one per sample timestamp and entity, each with its key fields:

| `sar.metricset` | Key fields |
|---|---|
| `cpu` | `sar.cpu.id`. Merges every per-CPU section for that CPU |
| `io` | `sar.io.device` |
| `network` | `sar.network.interface`. Merges the traffic and error sections |
| `filesystem` | `sar.filesystem.name`, plus `sar.filesystem.mountpoint` when reported |
| `interrupt` | `sar.interrupt.id`, plus `sar.interrupt.cpu` when per-CPU |
| `tty` | `sar.tty.name` |
| `fan`, `temp`, `in` | `sar.<kind>.name` = `DEVICE/N`, `sar.<kind>.device`, `sar.<kind>.id` |
| `usb` | `sar.usb.name` = `BUS/idvendor:idprod` |
| `fc_host` | `sar.fc_host.name` |

Per-CPU fields SHALL use the same names in `summary` (`all`) and `cpu` documents, so consumers SHALL filter on `sar.metricset` before aggregating them. No SAR document SHALL be written to any other data stream, including `metrics-node*`. Entity rows SHALL be separate flat documents, never arrays nested in the `summary` document.

#### Scenario: Host-wide sections merge per timestamp
- **GIVEN** a `sar.txt` whose CPU, memory, load, and socket sections all report a sample at the same timestamp
- **WHEN** the file is processed
- **THEN** exactly one document with `sar.metricset` = `summary` MUST exist for that timestamp, carrying `sar.cpu.*`, `sar.memory.*`, `sar.load.*`, and `sar.sockets.*` fields

#### Scenario: Entity documents share the stream and timestamp
- **GIVEN** a sample at `00:10:10` with 2 CPUs, 3 block devices, and 2 interfaces
- **WHEN** the file is processed
- **THEN** `metrics-host.sar-esdiag` MUST contain, at that `@timestamp`, 1 `summary`, 2 `cpu`, 3 `io`, and 2 `network` documents, and no other data stream MUST receive SAR documents

#### Scenario: Summary fields only on summary documents
- **GIVEN** processed SAR documents
- **WHEN** the user queries documents with `sar.metricset` other than `summary`
- **THEN** none MUST carry host-wide fields such as `sar.memory.*` or `sar.load.*`, and each MUST carry only its own section prefixes plus the shared host identity, correlation, and provenance fields

#### Scenario: Filesystem documents
- **GIVEN** a filesystem section reporting `/dev/mapper/root` and `/dev/sda1` with `FILESYSTEM` as the last column
- **WHEN** the section is parsed
- **THEN** each sample MUST yield one `filesystem` document per filesystem, with `sar.filesystem.name` set and `MBfsfree` exported as `sar.filesystem.fsfree.bytes`

#### Scenario: Sensor documents
- **GIVEN** a temperature section with rows for devices `coretemp-isa-0000` sensors `1` and `2`
- **WHEN** the section is parsed
- **THEN** each sample MUST yield one `temp` document per sensor, with `sar.temp.name` = `coretemp-isa-0000/1` or `coretemp-isa-0000/2` and `sar.temp.degc` set

#### Scenario: Per-device documents
- **GIVEN** a `DEV` section reporting 3 devices across 144 sample timestamps
- **WHEN** the file is processed
- **THEN** 432 `io` documents MUST be exported, each with one `sar.io.device` value

#### Scenario: Interface traffic and errors merge
- **GIVEN** a `sar.txt` with both the `IFACE` traffic section (`rxpck/s`) and the `IFACE` error section (`rxerr/s`) for interface `eth0` at the same timestamp
- **WHEN** the file is processed
- **THEN** exactly one `network` document MUST exist for `eth0` at that timestamp, carrying both `sar.network.rx.packets_per_sec` and `sar.network.rx.errors_per_sec`

#### Scenario: Template matches the stream
- **GIVEN** the ESDiag Elasticsearch assets
- **WHEN** the output data-stream naming contract test runs
- **THEN** `metrics-host.sar-esdiag` MUST have exactly one matching index template, and no `metrics-node*` template MUST match it

### Requirement: Host identity
Every SAR document SHALL carry the host identity parsed from the `sar.txt` header:
- `host.name`: the hostname inside the first parentheses, unmodified.
- `host.os.kernel`: the kernel release.
- `host.architecture`: the architecture without surrounding underscores, when present.
- `host.cpu.count`: the CPU count, when present.

When host-to-node correlation finds concrete listening IP addresses, `host.ip` SHALL list them.

#### Scenario: Header with FQDN, architecture, and CPU count
- **GIVEN** the header `Linux 4.18.0-553.97.1.el8_10.x86_64 (node-01.example.com) \t02/21/2026 \t_x86_64_\t(32 CPU)`
- **WHEN** the file is processed
- **THEN** every document MUST carry `host.name` = `node-01.example.com`, `host.os.kernel` = `4.18.0-553.97.1.el8_10.x86_64`, `host.architecture` = `x86_64`, and `host.cpu.count` = `32`

### Requirement: Co-located Elasticsearch node correlation
The system SHALL correlate a SAR host with zero, one, or many Elasticsearch nodes from the same diagnostic's node list.

Correlated nodes SHALL be recorded as multi-valued `elasticsearch.node.id` and `elasticsearch.node.name`. The method that produced the match SHALL be recorded in `elasticsearch.node.match`, one of `listen_socket`, `k8s_node_name`, `hostname`, `manifest_host`, or `none`.

The system SHALL apply these methods in order and use the first that matches at least one node:
1. **`listen_socket`**: the node's transport or HTTP publish `ip:port` equals a listening socket with a concrete, non-loopback address in `syscalls/ss.txt`, or in `syscalls/netstat.txt` when `ss.txt` is absent.
2. **`k8s_node_name`**: the node attribute `k8s_node_name` equals the SAR hostname, case-insensitively, by full name or by short name (the first label).
3. **`hostname`**: the SAR hostname equals the node's `host`, `name`, or HTTP publish hostname, case-insensitively, by full name; or, by short name, only when every short-name match shares the same full host value.
4. **`manifest_host`**: the collection target host recorded in the bundle manifest matches the node's `host`, `ip`, or HTTP publish hostname. A loopback or `localhost` target SHALL never match.

SAR documents SHALL NOT carry the single-node `node.*` enrichment fields.

#### Scenario: ECE allocator with multiple nodes
- **GIVEN** a host listening on `10.0.0.5:18001` and `10.0.0.5:18002`, and two nodes publishing transport addresses `10.0.0.5:18001` and `10.0.0.5:18002`
- **WHEN** the file is processed
- **THEN** every SAR document MUST carry both node IDs and both node names in `elasticsearch.node.id` and `elasticsearch.node.name`, and `elasticsearch.node.match` = `listen_socket`

#### Scenario: Other allocators' nodes are excluded
- **GIVEN** a diagnostic whose node list includes nodes on other hosts, with publish addresses this host does not listen on
- **WHEN** the file is processed
- **THEN** those nodes MUST NOT appear in `elasticsearch.node.*`

#### Scenario: Wildcard binds do not match
- **GIVEN** a host whose only Elasticsearch listening sockets are bound to `0.0.0.0` or `[::]`
- **WHEN** listening-socket correlation runs
- **THEN** no node MUST match by `listen_socket`, and the next method MUST be tried

#### Scenario: ECK Kubernetes node
- **GIVEN** a SAR hostname `worker-3` and two nodes with attribute `k8s_node_name` = `worker-3`, but no listening-socket match
- **WHEN** the file is processed
- **THEN** both nodes MUST be correlated with `elasticsearch.node.match` = `k8s_node_name`
- **AND** every SAR document MUST carry `kubernetes.node.name` = `worker-3`

#### Scenario: Ambiguous short hostname
- **GIVEN** a SAR hostname `es-01` and nodes with `host` values `es-01.dc1.example.com` and `es-01.dc2.example.com`
- **WHEN** hostname correlation runs
- **THEN** no node MUST be correlated by `hostname`

#### Scenario: No correlation evidence
- **GIVEN** a bundle whose node list is missing or matches by no method
- **WHEN** the file is processed
- **THEN** SAR documents MUST still be exported with `host.*` identity, `elasticsearch.node.match` = `none`, and no `elasticsearch.node.id` or `elasticsearch.node.name`

### Requirement: Sample timestamps
Each SAR document's `@timestamp` SHALL be the SAR sample time. `diagnostic.collection_date` SHALL remain the bundle collection time. SAR times without a timezone SHALL be interpreted as UTC, and every SAR document SHALL record `sar.timezone` = `assumed_utc`. The system SHALL NOT derive a timezone from `syscalls/uname.txt`.

The system SHALL accept row timestamps in these forms: `HH:MM:SS AM|PM`, `HH:MM:SS`, `YYYY-MM-DD HH:MM:SS AM|PM`, `YYYY-MM-DD HH:MM:SS`, and ISO 8601 `YYYY-MM-DDTHH:MM:SS`. Time-only rows take their date from the header report date.

#### Scenario: Per-sample timestamp
- **GIVEN** a header date `08/05/2026` and a CPU row at `12:10:10 AM`
- **WHEN** the file is processed
- **THEN** the `summary` document's `@timestamp` MUST be `2026-08-05T00:10:10Z`, and `diagnostic.collection_date` MUST equal the manifest collection date

#### Scenario: Midnight rollover within a section
- **GIVEN** a section whose time-only rows go from `11:50:00 PM` to `12:00:01 AM`
- **WHEN** the section is parsed
- **THEN** the `12:00:01 AM` row MUST be dated one day after the `11:50:00 PM` row

#### Scenario: Rollover resets at each section
- **GIVEN** a CPU section spanning `12:10 AM` to `11:50 PM`, followed by a memory section starting at `12:10 AM`
- **WHEN** the file is processed
- **THEN** the memory section's first row MUST share the date and `@timestamp` of the CPU section's first row

#### Scenario: Ambiguous day and month
- **GIVEN** a header date `04/08/2026` and a manifest collection date of `2026-08-05`
- **WHEN** the header is parsed
- **THEN** the report date MUST be `2026-08-04`, the candidate on or before the collection date and closest to it

#### Scenario: Unambiguous day-first date
- **GIVEN** a header date `21/02/2026`
- **WHEN** the header is parsed
- **THEN** the report date MUST be `2026-02-21`

#### Scenario: Two-digit year across new year
- **GIVEN** a header date `12/31/25` and a collection date of `2026-01-01`
- **WHEN** the header is parsed
- **THEN** the report date MUST be `2025-12-31`

### Requirement: Header-driven column parsing
The system SHALL resolve each value by the column name in the current section's header, never by a fixed position. It SHALL accept known sysstat aliases for the same metric:
- `%user` for `%usr`.
- `rd_sec/s` and `wr_sec/s` (512-byte sectors) for `rkB/s` and `wkB/s`.
- `avgrq-sz` (sectors) for `areq-sz` (kilobytes).
- `avgqu-sz` for `aqu-sz`.

A column absent in a given sysstat version SHALL leave its field absent, not zero.

#### Scenario: Older sysstat device columns
- **GIVEN** a `DEV` section with columns `tps rd_sec/s wr_sec/s avgrq-sz avgqu-sz await svctm %util` and a row with `rd_sec/s` = `8.00`
- **WHEN** the section is parsed
- **THEN** `sar.io.read.bytes_per_sec` MUST be `4096.0`

#### Scenario: Newer sysstat without svctm
- **GIVEN** a `DEV` section without an `svctm` column
- **WHEN** the section is parsed
- **THEN** `io` documents MUST omit `sar.io.svctm_ms` and carry every other present column

#### Scenario: Older CPU column name
- **GIVEN** a CPU section header using `%user`
- **WHEN** the section is parsed
- **THEN** `sar.cpu.usr.pct` MUST be populated from that column

### Requirement: Unit normalization
The system SHALL name and normalize SAR values with these generic rules. They apply to every column, including columns this change does not list:
- Names are lowercased, and `-` becomes `_`.
- Percentage columns (`%x`) become ratios, the value divided by 100, in `x.pct`. They SHALL NOT be clamped, because `%vmeff` can exceed 100.
- Kilobyte columns (`kbx`) become bytes in `x.bytes`, multiplied by 1024.
- Megabyte columns (`MBx`) become bytes in `x.bytes`, multiplied by 1048576.
- Kilobyte-rate columns (`xkB/s`) become `x.bytes_per_sec`, multiplied by 1024.
- Sector-rate columns (`rd_sec/s`, `wr_sec/s`) become `read.bytes_per_sec` and `write.bytes_per_sec`, multiplied by 512.
- Other rate columns (`x/s`) become `x_per_sec`.
- Latency columns become fields ending `_ms`.

Non-numeric columns (entity names, USB vendor and product identifiers, descriptions) SHALL be kept as keywords. USB manufacturer and product text can contain spaces, so it SHALL be kept whole as `sar.usb.description`.

No indexed SAR field name SHALL match an ESDiag dynamic-template pattern that disables or re-types fields: `*_time`, `*time_string`, `*size`, `*_memory`, `*histogram`, `*timestamp`, `*date_millis`, or `*unsafe*`. A generated name that would match one SHALL get `_value` appended.

#### Scenario: Percentage ratio
- **GIVEN** a CPU row with `%usr` = `6.10`
- **WHEN** the row is parsed
- **THEN** `sar.cpu.usr.pct` MUST be `0.061`

#### Scenario: VM efficiency above 100
- **GIVEN** a paging row with `%vmeff` = `141.31`
- **WHEN** the row is parsed
- **THEN** `sar.paging.vmeff.pct` MUST be `1.4131`

#### Scenario: Kilobytes to bytes
- **GIVEN** a memory row with `kbmemfree` = `5523262`
- **WHEN** the row is parsed
- **THEN** `sar.memory.memfree.bytes` MUST be `5655820288`

### Requirement: Section recognition and robustness
The system SHALL identify sections by the distinctive columns listed under "Complete section coverage", never by a leading entity column alone. Sections sharing `CPU`, `IFACE`, or `INTR` leading columns SHALL be told apart by their metric columns.

The system SHALL:
- Strip ANSI CSI escape sequences, carriage returns, and NUL bytes from every line before parsing.
- Skip rows whose prefix is not a valid timestamp, including `Average:` and its localized forms.
- Ignore `LINUX RESTART` lines without disturbing section or rollover state.
- Never attribute a row of an unrecognized section to the previous section.
- Never hard-code device, interface, filesystem, sensor, or CPU names or counts.

#### Scenario: ANSI-decorated input
- **GIVEN** a `sar.txt` whose lines contain ANSI color and cursor escape sequences, `\r`, and NUL bytes
- **WHEN** the file is processed
- **THEN** the exported values MUST equal those parsed from the same file without control bytes

#### Scenario: Softnet does not overwrite CPU
- **GIVEN** a softnet section (`CPU total/s dropd/s ...`) following the CPU utilization section
- **WHEN** the file is processed
- **THEN** `sar.cpu.*` utilization values MUST come only from the CPU utilization section, and softnet values MUST appear under `sar.softnet.*` in the `summary` document (`all`) and the `cpu` documents (numbered CPUs)

#### Scenario: Per-interrupt matrix
- **GIVEN** an interrupt section whose header has one `N/s` column per interrupt and one row per CPU
- **WHEN** the section is parsed
- **THEN** each sample MUST yield one `interrupt` document per interrupt and CPU, with `sar.interrupt.id` = the interrupt number, `sar.interrupt.cpu` = the CPU, and `sar.interrupt.intr_per_sec` = the cell value

#### Scenario: Restart mid-section
- **GIVEN** a CPU section containing `03:14:22 AM LINUX RESTART (32 CPU)` followed by a repeated CPU header
- **WHEN** the section is parsed
- **THEN** rows before and after the restart MUST be exported with correct timestamps, and no document MUST be produced for the restart line

#### Scenario: Summary rows ignored
- **GIVEN** sections ending with `Average:` rows
- **WHEN** the file is processed
- **THEN** no document MUST be produced from any `Average:` row

### Requirement: SAR parse outcomes
SAR SHALL map its parse results onto the `syscall-processing` failure policy:
- Content with no recognizable `sar` file header, or a header with no data rows (for example shell error text), SHALL be a parse failure: one warning event, no SAR documents.
- Correlation inputs (the `ss` and `netstat` catalog entries and the node list) SHALL be optional. When one is missing or unreadable, only the correlation methods that depend on it SHALL be skipped, with no warning event, and SAR documents SHALL still be exported.

#### Scenario: sar not installed
- **GIVEN** `syscalls/sar.txt` containing `bash: sar: command not found`
- **WHEN** the diagnostic is processed
- **THEN** no SAR documents MUST be exported and the report MUST record one SAR warning naming the missing header

#### Scenario: Correlation inputs missing
- **GIVEN** a bundle with valid `syscalls/sar.txt` but no `syscalls/ss.txt` or `syscalls/netstat.txt`
- **WHEN** the diagnostic is processed
- **THEN** SAR documents MUST be exported, correlation MUST continue with the remaining methods, and no warning MUST be recorded for the missing inputs
