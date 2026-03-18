## 1. Assets and Source Catalog

- [x] 1.1 Create `assets/agent/sources.yml` with file path definitions for agent-info, state, computed-config, local-config, and per-component sources
- [x] 1.2 Add `"agent"` to the `embedded_sources_str` match in `data_source.rs`
- [x] 1.3 Add `"agent"` to the product loop in `load_embedded_sources`
- [x] 1.4 Add `"agent"` required source key (`agent_info`) to `required_source_keys`

## 2. Manifest Synthesis

- [x] 2.1 Add `agent-info.yaml` fallback to the `try_get_manifest_from_files` chain in `src/receiver/mod.rs` — parse YAML, construct `DiagnosticManifest` with `Product::Agent`, version from `metadata.elastic.agent.version`, collection date from archive directory name, name from `metadata.host.hostname`

## 3. Data Structures

- [x] 3.1 Create `src/processor/agent/` module directory
- [x] 3.2 Implement `src/processor/agent/agent_info.rs` — Serde model for `agent-info.yaml` with `metadata.elastic.agent.*`, `metadata.host.*`, `metadata.os.*`; `DataSource` impl
- [x] 3.3 Implement `src/processor/agent/state.rs` — Serde model for `state.yaml` (top-level state/message/fleet_state + `components[]` with id/state/message/units); `DataSource` + `DocumentExporter` that splits into agent-level + per-component docs → `metrics-agent.state-esdiag`
- [x] 3.4 Implement `src/processor/agent/computed_config.rs` — `DataSource` + `DocumentExporter` using `serde_json::Value` passthrough → `settings-agent.computed_config-esdiag`
- [x] 3.5 Implement `src/processor/agent/local_config.rs` — `DataSource` + `DocumentExporter` → `settings-agent.local_config-esdiag`
- [x] 3.6 Implement `src/processor/agent/beat_metrics.rs` — `DataSource` + `DocumentExporter` for `components/*/beat_metrics.json` with `component` field injected from subdirectory name → `metrics-agent.beat-esdiag`
- [x] 3.7 Implement `src/processor/agent/input_metrics.rs` — `DataSource` + `DocumentExporter` that collects `input_metrics.json` from all components, splits each array into one doc per input entry → consolidated `metrics-agent.input-esdiag` (entries self-identify via `input` and `id` fields); define `component` field alias → `input` in the data stream mapping
- [x] 3.8 Implement `src/processor/agent/rendered_config.rs` — `DataSource` + `DocumentExporter` for `components/*/beat-rendered-config.yml`; split by top-level key: `inputs` (split array) → `settings-agent.inputs-esdiag`, `outputs` → `settings-agent.outputs-esdiag`, `features` → `settings-agent.features-esdiag`, `apm` → `settings-agent.apm-esdiag`; inject `component` field from subdirectory name; nested arrays deeper than first level use `flattened` field type
- [x] 3.9 Implement `src/processor/agent/logs.rs` — `DocumentExporter` for `logs/**/*.ndjson` — enumerate ndjson files, forward each line with metadata enrichment → `logs-elastic.agent-esdiag`

## 4. Metadata

- [x] 4.1 Implement `src/processor/agent/metadata.rs` — `AgentMetadata` struct built from `AgentInfo` + `DiagnosticManifest`; contains `agent.*`, `host.*`, `os.*`, and `diagnostic.*` fields
- [x] 4.2 Implement `Metadata` trait on `AgentMetadata` (serialize as meta doc with `@timestamp`, agent identity, `diagnostic.*`)

## 5. Diagnostic Processor

- [x] 5.1 Implement `src/processor/agent/mod.rs` — `AgentDiagnostic` struct with `Lookups`, `AgentMetadata`, `receiver`, `exporter`
- [x] 5.2 Implement `DiagnosticProcessor::try_new` — fetch `agent-info.yaml`, build metadata and report, return `(Box<AgentDiagnostic>, DiagnosticReport)`
- [x] 5.3 Implement `DiagnosticProcessor::process` — enumerate root data sources + discover `components/` subdirectories dynamically + process logs
- [x] 5.4 Implement `DiagnosticProcessor::origin` — return `(hostname, agent_id, "agent")`

## 6. Dispatcher Integration

- [x] 6.1 Add `mod agent;` and `use agent::AgentDiagnostic;` to `src/processor/mod.rs`
- [x] 6.2 Add `Diagnostic::Agent(Box<AgentDiagnostic>)` variant to the `Diagnostic` enum
- [x] 6.3 Implement `Diagnostic::uuid`, `Diagnostic::try_new`, `Diagnostic::process`, and `Diagnostic::origin` arms for `Agent`
- [x] 6.4 Add `Product::Agent => AgentDiagnostic::try_new(...)` branch in `Diagnostic::try_new`

## 7. Verification

- [ ] 7.1 Write integration test covering end-to-end processing of an Agent diagnostic archive (using clean archives in tests/archives/)
- [x] 7.2 Run `cargo clippy -- -D warnings` and resolve all warnings
- [x] 7.3 Run `cargo test` and confirm all tests pass
