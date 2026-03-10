## Why

PR260 brought full support-diagnostic API collection to Elasticsearch by treating `sources.yml` as the source of truth for what a support run should fetch. Logstash collection still only resolves a small hardcoded subset, so `esdiag collect logstash --type support` does not yet match the legacy diagnostic output described by `assets/logstash/sources.yml`.

## What Changes

- Load `assets/logstash/sources.yml` into the shared source configuration so Logstash endpoints can be resolved dynamically by version, file path, and output extension.
- Move `sources.yml` product selection into the active receiver or collect command context so each execution resolves files and URLs from exactly one product registry via a shared source context.
- Expand Logstash API selection so `support` includes every top-level Logstash source, while `light` and `standard` continue to resolve a bounded subset appropriate for lighter-weight runs.
- Add generic raw Logstash collection for endpoints that do not have typed processors, including `logstash_health_report`, `logstash_nodes_hot_threads`, `logstash_nodes_hot_threads_human`, `logstash_plugins`, and `logstash_version`.
- Preserve existing typed Logstash processing for `logstash_node` and `logstash_node_stats`, avoiding duplicate fetches when a typed processor already covers a selected source.
- Add dedicated Logstash client and receiver implementations instead of routing Logstash traffic through the Elasticsearch transport.
- Split bundle metadata files like `manifest.json` and `diagnostic_manifest.json` away from `DataSource` so only `sources.yml`-backed APIs participate in source resolution.
- Record the resolved Logstash API list in the diagnostic manifest and keep include/exclude validation aligned with the keys defined in `assets/logstash/sources.yml`.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `api-selection`: Update Logstash diagnostic type resolution and include/exclude validation to use the Logstash `sources.yml` keys instead of a hardcoded enum subset.
- `support-diagnostics-parity`: Extend full support-diagnostic parity to Logstash by fetching and storing all selected Logstash endpoints, using raw collection where no typed processor exists.
- `version-dependent-sources`: Extend dynamic source loading and path/URL resolution to support product-specific Logstash source definitions in addition to Elasticsearch.

## Impact

- **Core processing logic**: Affects `src/processor/api.rs`, Logstash diagnostic orchestration, client/receiver dispatch, generic receiver/export flow for raw endpoints, and explicit bundle-file manifest loading.
- **Assets/configuration**: Introduces first-class use of `assets/logstash/sources.yml` at runtime.
- **Diagnostic output**: Support collections will contain additional Logstash `.json` and `.txt` outputs, increasing parity with legacy support diagnostics.
