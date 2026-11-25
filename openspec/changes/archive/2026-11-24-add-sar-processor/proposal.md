## Why

Elasticsearch `support-diagnostics` bundles that run system calls include `syscalls/sar.txt`: roughly 24 hours of `sar -A` host activity at 10-minute intervals. It covers per-core CPU, memory, paging, swap, load, block and per-device I/O, filesystems, network interfaces and protocols, sockets, NFS, interrupts, and power sensors. ESDiag ignores it today, so correlating an incident with host saturation still needs external tools such as kSar or ad-hoc scripts. Indexing it gives a 24-hour host timeline next to the cluster data from the same diagnostic.

`sar` measures a **host**, not an Elasticsearch node. Self-managed hosts often run one node, but an ECE allocator runs several nodes on one IP with different ports, and an ECK Kubernetes node runs several pods. The data model must be host-scoped, with co-located Elasticsearch nodes attached as a correlation, never as a 1:1 identity.

`sar.txt` is the first of many `support-diagnostics` OS-command outputs (`diags.yml`: `top`, `ss`, `netstat`, `iostat`, `sysctl`, `dmesg`, and others). SAR therefore needs a processing foundation that later syscall processors and a later syscall collection change can reuse, not a one-off hook.

## What Changes

- A generic syscall processing layer:
  - An ESDiag-owned syscall source catalog (`assets/syscalls/sources.yml`), keyed by `diags.yml` command keys, separate from the REST `sources.yml`. It defines each output file and its diagnostic-type tags, processability, and implementing applications. A future collection change can add per-OS commands to it.
  - Each processable syscall source is an opt-out processing option, `syscalls.<key>`, in the existing processing selection. `syscalls.sar` is on by default for `standard` and `support`, off for `minimal` and `light`.
  - The processing selection records which keys the user explicitly requested, distinct from defaults.
  - One missing-file and failure policy for every syscall source. A source selected by default that is missing is skipped silently. An explicitly requested source that is missing is a warning. Parse failures on an existing file are always warnings. No syscall source ever fails the diagnostic or counts as a processor failure.
- New SAR processor for Elasticsearch diagnostic bundles, the first syscall processor: when `syscalls.sar` is selected and `syscalls/sar.txt` exists, stream-parse it and export host-scoped documents. Processing is gated on selection and the file, not on `Platform`, because allocator and Kubernetes-node collections also carry `syscalls/`.
- Ingest everything in the file: every section `sar -A` can emit, every row including per-core CPU and idle devices, and every column including ones from newer sysstat versions (generic naming rules). A section the parser does not recognize produces a report warning naming its columns, so nothing is dropped silently.
- One new host-scoped data stream, `metrics-host.sar-esdiag`, with one index template. Every document carries `@timestamp` and a `sar.metricset` keyword:
  - `summary`: exactly one per sample timestamp, carrying all the host-wide metrics (CPU `all`, memory, paging, swap, load, sockets, protocols, NFS, and so on).
  - Entity documents at the same timestamps: `cpu` (per core), `io` (per block device), `network` (per interface, traffic and errors merged), `filesystem`, `interrupt`, `tty`, `fan`, `temp`, `in`, `usb`, and `fc_host`. Each carries only its own section fields plus its entity key.
  - Dashboards and queries select a kind by filtering on `sar.metricset`.
- A new `esdiag@host` component template for `host.*` identity fields and the co-located `elasticsearch.node.*` correlation fields.
- Host-to-node correlation that can attach zero, one, or many Elasticsearch nodes to a host. Evidence, in priority order: listening sockets in `syscalls/ss.txt` (fallback `syscalls/netstat.txt`) matched against node publish addresses; the ECK `k8s_node_name` node attribute; the hostname; the manifest's collection host input.
- Per-sample `@timestamp` from the SAR row (assumed UTC, recorded on the document), while `diagnostic.collection_date` stays the bundle collection time. This adds a timestamp-override constructor to the Elasticsearch metadata envelope.
- Tolerant parsing: header-driven columns with sysstat version aliases, ANSI/control-byte stripping, `Average:` and `LINUX RESTART` lines skipped, per-section midnight rollover, and day/month disambiguation against the collection date.
- A derived `sar.io.device_class` (`sd`, `dm`, `nvme`, …) on block-device documents, from kernel names or major numbers, for filtering.
- Kibana: one data view and four cross-linked **Host Activity** dashboards modeled on kSar (Compute, Storage, Network, System), with one collapsible section per kSar graph and a links panel also added to Hosts Overview.
- Docs: update the `CONTEXT.md` "Infrastructure data" definition (host system calls can come from allocator or Kubernetes-node collections) and `CHANGELOG.md`.

Out of scope:
- `Average:` summary rows. Averages can be recomputed from the samples.
- Binary sysstat data files (`saDD`, `sarDD`). Only the `sar -A` text output in `syscalls/sar.txt` is processed.
- A timezone override.
- Collecting syscall output (running commands, per-OS command definitions, reconciling the catalog with `diags.yml`). That is a separate syscall collection change.
- Syscall processors for Kibana and Logstash bundles, and processors for syscall files other than `sar.txt`.

## Capabilities

### New Capabilities
- `syscall-processing`: The syscall source catalog, `syscalls.<key>` processing options with default and explicit selection, the shared missing-file and failure policy, and platform independence. Applies to every current and future syscall processor.
- `host-sar-processing`: Parsing `syscalls/sar.txt` and exporting host-scoped SAR metrics to the `metrics-host.sar-esdiag` data stream. Covers sections, units, timestamps, host-to-node correlation, and how SAR parse results map onto the syscall failure policy.

### Modified Capabilities
- `kibana-dashboards`: Adds the Host Activity Compute, Storage, Network, and System dashboards over the new host data stream.

## Impact

- **Product**: Elasticsearch diagnostics (`support-diagnostics` bundles with `syscalls/`). Kibana and Logstash bundles are unaffected.
- **Surface**: Core processing and selection, plus Elasticsearch/Kibana assets. No new CLI flags. The web advanced processing options and saved-job selections gain the `syscalls.sar` option through the existing option catalog, with no template changes. New data streams appear in Kibana after `esdiag setup`.
- **Code**: A syscall catalog loader and registry validation. `ApiResolver` gains `syscalls.*` options, tag defaults, collect-validation exemption, and `ProcessSelection.requested` (additive, `serde(default)`). A shared syscall runner. A new `processor/elasticsearch/sar/` module, dispatched through the Elasticsearch syscall table. `Lookups` gains node endpoint data derived from the already-parsed `Nodes` source. `ElasticsearchMetadata` gains `for_data_stream_at`.
- **Assets**:
  - `assets/syscalls/sources.yml`.
  - `assets/elasticsearch/component_templates/esdiag@host.json`.
  - `assets/elasticsearch/index_templates/metrics-host.sar.json`.
  - Kibana data view, four dashboards, and a links object under `assets/kibana/esdiag/objects/`, plus a links panel on Hosts Overview in place of its Nodes Overviews links. Hosts Overview moves from the Nodes Overviews links to the end of the Elasticsearch overview links.
- **Dependencies**: none new. Timestamp parsing uses the existing date crate.
- **Volume**: 144 samples per day, times one `summary` document plus one document per CPU, device, interface, filesystem, and sensor. The sample bundle (4 CPUs, 19 devices, 2 interfaces) yields about 3.7k documents. A 64-core host with per-interrupt data can reach hundreds of thousands, which is still bounded by samples × entities.
