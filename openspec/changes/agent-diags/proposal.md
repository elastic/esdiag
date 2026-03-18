## Why

Elastic Agent diagnostics contain rich operational data — policy, component health, log streams, and inspect output — that ESDiag does not yet process. Surfacing this data in Elasticsearch alongside existing Elasticsearch and Logstash diagnostics gives support engineers a unified view of the full observability stack.

## What Changes

- Add `AgentDiagnostic` processor implementing the `DiagnosticProcessor` trait for Elastic Agent diagnostic bundles
- Parse and export Agent-specific data sources: component state, policy, version, logs, and inspect output
- Register `agent` as a supported `--type` value in the CLI `collect` command
- Add Agent diagnostic type to the `api-selection` capability (source definitions and filtering)

## Capabilities

### New Capabilities

- `agent-diagnostic-collection`: Processing pipeline for Elastic Agent diagnostic bundles — data source definitions, Serde models, document export, and lookup enrichment for component state

### Modified Capabilities

- `api-selection`: Add `agent` as a recognized diagnostic type with its data sources and include/exclude filtering support

## Impact

- **New module**: `src/processor/agent/` — mirrors structure of `src/processor/elasticsearch/` and `src/processor/logstash/`
- **CLI**: `collect --type agent` becomes a valid invocation
- **Exports**: New documents in Elasticsearch data streams under the `agent` namespace
- **No breaking changes** to existing diagnostic types or collection flows
