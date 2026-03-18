## Context

ESDiag processes diagnostic bundles from Elasticsearch, Logstash, ECK, and Kubernetes Platform. Each product follows the same pattern: a `src/processor/{product}/` module implementing `DiagnosticProcessor`, keyed by `Product` enum variant, with a `Diagnostic` enum arm in `src/processor/mod.rs` dispatching to it.

`Product::Agent` already exists in `src/data/product.rs` and parses from `"agent"`. However, there is no `agent` processor module, no `assets/agent/sources.yml`, and no `Diagnostic::Agent` arm — so the dispatcher falls through to `Err("Unsupported product or diagnostic bundle")`.

The `embedded_sources_str` function and the `load_embedded_sources` loop in `data_source.rs` are hard-coded to `["elasticsearch", "kibana", "logstash"]`. Agent must be registered there as well.

### Bundle origin

Agent diagnostics are **not collected via REST API**. Unlike Elasticsearch, Logstash, and Kibana — which expose HTTP endpoints that ESDiag calls directly — Agent diagnostics are generated locally by the `elastic-agent diagnostics` CLI command. The command produces a zip archive (`elastic-agent-diagnostics-<timestamp>.zip`) containing a snapshot of the agent's runtime state, configuration, metrics, profiles, and logs.

This means:
- There is no live `collect` path for Agent — only `process` of pre-collected archives.
- The `sources.yml` / `DataSource` model (designed for version-gated REST URLs) is repurposed here purely as a **filename→extension lookup table**. The `versions` entries use a catch-all `">= 0.0.0": ""` since there are no URLs to resolve.
- The `--exclude-events` CLI flag controls whether `events/` log files are included in the archive. Our processor excludes them regardless (they contain sensitive data).

**Credentials warning:** The official docs note that credentials may not be redacted in the archive — they may appear in plain text in configuration or policy files. This is confirmed in `beat-rendered-config.yml` component files.

### Bundle structure

The Agent diagnostic bundle is structurally different from all other diagnostics:

- **No `manifest.json` or `diagnostic_manifest.json`** — the bootstrap identity file is `agent-info.yaml`
- **Predominantly YAML** rather than JSON at the root level
- **Three-level hierarchy** — root files describe the agent, `components/{id}/` subdirectories describe each managed Beat, `edot/` contains OTel collector diagnostics
- **`logs/` directory** contains rotated `.ndjson` log files (agent logs, event logs, HTTP request traces)
- **Collection timestamp** must be derived from the archive directory name (e.g. `elastic-agent-diagnostics-YYYY-MM-DDTHH-MM-SSZ-NN`)

### Skipped file types

All `*.gz`, `*.txt`, and `*.tar.gz` files are out of scope for this first round.

## Goals / Non-Goals

**Goals:**
- Implement a complete `AgentDiagnostic` processor following the established module pattern
- Define `assets/agent/sources.yml` with Agent data sources
- Register `agent` in `data_source.rs` so source loading and API selection work end-to-end
- Add `Diagnostic::Agent` arm to the dispatcher
- Export Agent documents to appropriately-namespaced data streams
- Synthesize a `DiagnosticManifest` from `agent-info.yaml` (since no manifest file exists)

**Non-Goals:**
- `AgentCollector` — Agent has no REST API for diagnostics. Bundles are generated locally by `elastic-agent diagnostics` CLI. There is nothing to collect from remotely; only pre-collected archives can be processed.
- OTel collector diagnostics (`edot/`, `otel-merged.yaml`) — deferred to OTel-specific work
- `environment.yaml` processing — may contain sensitive values, needs redaction strategy first
- Modeling the `components-expected.yaml` / `components-actual.yaml` protobuf-style nested structures (deferred)
- UI changes or new Kibana dashboards

## File → Data Stream Mapping

### Root-level files

| File | Data Stream | Transform | Notes |
|------|-------------|-----------|-------|
| `agent-info.yaml` | _(identity source)_ | Metadata extraction only | Populates `agent.*` and `host.*` fields on all exported docs. Not exported as its own document. |
| `state.yaml` | `metrics-agent.state-esdiag` | +metadata, **split** `components[]` | 1 agent-level doc (fleet_state, message, state) + 1 doc per component (id, state, message, units). Component docs include unit-level health. |
| `computed-config.yaml` | `settings-agent.computed_config-esdiag` | +metadata, **RawValue** passthrough | Single doc. Schema is too large/variable to model — store as opaque `serde_json::Value`. Contains redacted secrets. |
| `local-config.yaml` | `settings-agent.local_config-esdiag` | +metadata | Single doc. Agent-level settings: logging, monitoring, fleet, grpc, upgrade watcher. |
| `variables.yaml` | _(skip)_ | — | Empty in practice (`variables: [{}]`). No operational value. |
| `pre-config.yaml` | _(skip, v1)_ | — | Pre-variable-substitution config (the raw `elastic-agent.yaml` from disk or Fleet). Defer until diff analysis against `computed-config.yaml` is needed. |
| `otel.yaml` | _(skip)_ | — | OTel collector config. Text sentinel when inactive. Defer to OTel-specific processing. |
| `otel-merged.yaml` | _(skip)_ | — | Final merged OTel collector config including internal components. Defer to OTel-specific processing. |
| `environment.yaml` | _(skip, v1)_ | — | Environment variables visible to the agent process. May contain sensitive values. |
| `components-expected.yaml` | _(skip, v1)_ | — | Protobuf-style `structvalue/kind/numbervalue` nesting is impractical to model. State.yaml covers operational health. |
| `components-actual.yaml` | _(skip, v1)_ | — | Same as expected. Defer until expected-vs-actual comparison is needed. |

### Per-component files (`components/{component-id}/`)

Each component directory corresponds to one supervised process (typically one input-output pair). The docs note these contain `*_metrics.json` files and `*.pprof.gz` profiles.

| File | Data Stream | Transform | Notes |
|------|-------------|-----------|-------|
| `beat_metrics.json` | `metrics-agent.beat-esdiag` | +metadata, +`component` | 1 doc per component. `component` field injected with the subdirectory name. Deep JSON with `beat.*`, `libbeat.*`, `system.*`, `registrar.*` stats. RawValue passthrough. |
| `input_metrics.json` | `metrics-agent.input-esdiag` | +metadata, **split** array | 1 doc per input entry. Each entry self-identifies via `input` (component type) and `id` (stream ID) fields. Entries from all components consolidated into a single data stream. Mapping includes an `agent.component` alias to `input` so `component` field filters work across both `beat` and `input` data streams. |
| `beat-rendered-config.yml` | Split by top-level key (see below) | +metadata, +`component`, **split** top-level keys | Top-level keys (`inputs`, `outputs`, `features`, `apm`) each route to a dedicated data stream. Nested arrays deeper than the first level use `flattened` field type. |

#### Rendered config data streams (from `beat-rendered-config.yml`)

| Top-level key | Data Stream | Transform | Notes |
|---------------|-------------|-----------|-------|
| `inputs` | `settings-agent.inputs-esdiag` | +metadata, +`component`, **split** array | 1 doc per input entry. Each input has `type`, `id`, `data_stream`, `processors`, etc. Nested arrays (e.g. `processors`, `streams`) use `flattened`. |
| `outputs` | `settings-agent.outputs-esdiag` | +metadata, +`component` | 1 doc per output entry. Keyed by output name (e.g. `elasticsearch`). Nested config uses `flattened`. |
| `features` | `settings-agent.features-esdiag` | +metadata, +`component` | 1 doc per component. Typically small (e.g. `fqdn.enabled`). |
| `apm` | `settings-agent.apm-esdiag` | +metadata, +`component` | 1 doc per component. Often empty (`{}`). |

### OTel collector diagnostics (`edot/`)

| File | Data Stream | Transform | Notes |
|------|-------------|-----------|-------|
| `edot/otel-merged-actual.yaml` | _(skip, v1)_ | — | Running OTel collector config. Defer to OTel-specific processing. |
| `edot/*.profile.gz` | _(skip)_ | — | Go pprof profiles. Out of scope for this round. |

### Log files (`logs/`)

| Pattern | Data Stream | Transform | Notes |
|---------|-------------|-----------|-------|
| `logs/**/*.ndjson` | `logs-elastic.agent-esdiag` | +metadata per line | Forwarded as-is. Each NDJSON line is already structured JSON with `@timestamp`, `log.level`, `message`. Metadata enrichment adds `agent.*` and `diagnostic.*` fields. Includes agent logs and HTTP request traces. Files under `events/` directories are excluded (sensitive data; corresponds to `--exclude-events` CLI flag). |

## Metadata Enrichment

Every exported document is enriched with two field groups:

### `agent.*` — from `agent-info.yaml`

```yaml
agent:
  id: "<agent-uuid>"
  version: "<semver>"
  snapshot: false
  unprivileged: true
host:
  hostname: "<hostname>"
  name: "<hostname>"
  arch: "x86_64"
  ip: ["127.0.0.1/8", "<container-ip>/16"]
os:
  family: "<os-family>"
  name: "<os-name>"
  platform: "<os-platform>"
  version: "<os-version>"
  kernel: "<kernel-version>"
```

### `diagnostic.*` — synthesized from `agent-info.yaml` + archive name

```yaml
diagnostic:
  id: "<hostname>@<date>~<uuid[:4]>"   # human-readable
  uuid: "<generated>"
  collection_date: 1742076428000                  # millis, parsed from dir name
  product: "agent"
```

### `@timestamp`

Set to `diagnostic.collection_date` for all exported docs except log lines, which retain their original `@timestamp`.

## Decisions

### Manifest synthesis from `agent-info.yaml`

The `try_get_manifest_from_files` chain (`diagnostic_manifest.json` → `manifest.json`) will fail for Agent bundles. A third fallback reads `agent-info.yaml` and constructs a `DiagnosticManifest` with:
- `product: Agent`
- `version`: from `metadata.elastic.agent.version`
- `collection_date`: parsed from the archive directory name
- `name`: from `metadata.host.hostname`

This keeps the processor lifecycle (`Processor<Ready>` → `Processor<Processing>`) unchanged.

### Module structure mirrors Logstash (not Elasticsearch)

Logstash is the closest analog in complexity. Like Agent, it has a manageable set of data sources, a simple `Lookups` struct, and no streaming data sources.

**Structure:**
```
src/processor/agent/
  mod.rs              — AgentDiagnostic, Lookups, DiagnosticProcessor impl
  metadata.rs         — AgentMetadata from agent-info.yaml
  agent_info/         — DataSource + serde model for agent-info.yaml
  state/              — DataSource + DocumentExporter for state.yaml (split components)
  computed_config/    — DataSource + DocumentExporter for computed-config.yaml (RawValue)
  local_config/       — DataSource + DocumentExporter for local-config.yaml
  beat_metrics/       — DataSource + DocumentExporter for components/*/beat_metrics.json
  input_metrics/      — DataSource + DocumentExporter for components/*/input_metrics.json (split array)
  rendered_config/    — DataSource + DocumentExporter for components/*/beat-rendered-config.yml; splits top-level keys (inputs, outputs, features, apm) into separate data streams
  logs/               — DocumentExporter for logs/**/*.ndjson (metadata enrichment only)
```

### RawValue passthrough for large/variable schemas

`computed-config.yaml`, `beat_metrics.json`, and `beat-rendered-config.yml` have schemas that vary by agent version, Beat type, and input configuration. Rather than modeling hundreds of fields, deserialize to `serde_json::Value` and pass through. This mirrors the `RawValue` pattern used in the Elasticsearch node stats processor.

### `Lookups` starts minimal

Agent has no cross-source enrichment needs at launch. `Lookups` will be a unit-like struct `Lookups {}` that can be extended later (e.g. component count from state.yaml) without trait-signature changes.

### `origin()` returns agent hostname and agent ID

Returns `(hostname, agent_id, "agent")` — consistent with `LogstashDiagnostic::origin()` pattern.

### Component discovery is dynamic

The `components/` subdirectory contains a variable number of component directories (one per managed Beat). The processor must enumerate component directories at runtime rather than hard-coding names. Each component directory is processed identically.

## Risks / Trade-offs

- **No manifest file** → Synthesis from `agent-info.yaml` adds a new fallback path in the receiver. If `agent-info.yaml` is missing or malformed, the processor cannot start. Mitigated by making `agent-info.yaml` the single required source key.
- **Collection date from directory name** → Fragile if the naming convention changes across agent versions. The regex should be lenient and fall back to current time if parsing fails.
- **RawValue passthrough** → Large `computed-config.yaml` docs could be 100KB+. This may cause mapping explosions in Elasticsearch if the config contains deeply nested dynamic fields. Consider a max-depth flattening or explicit `enabled: false` on the data stream mapping.
- **Log volume** → A single diagnostic can contain 70MB+ of NDJSON logs across rotated files. The streaming `DocumentExporter` pattern (not batch) should be used for log forwarding to avoid loading all logs into memory.
- **Unredacted secrets in `beat-rendered-config.yml`** → Component rendered configs may contain unredacted credentials (observed: API keys, client secrets). The processor should apply the same secret redaction that the agent applies to `computed-config.yaml`, or document this as a known limitation.
