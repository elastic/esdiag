## Why

ESDiag currently lacks support for processing Kibana-specific diagnostic data (like Kibana node stats or settings). Adding Kibana processors will allow users to import and analyze Kibana diagnostics in Elasticsearch, enabling better troubleshooting of the entire Elastic Stack.

## What Changes

- Add a new `kibana` processor module in `src/processor`.
- Implement specific processors for key Kibana diagnostic APIs.
- Update the diagnostic bundle detection to recognize and route Kibana files to the new processors.

## Capabilities

### New Capabilities
- `kibana-node-stats`: Process Kibana node statistics (e.g., memory usage, event loop latency) from `kibana_stats.json`.
- `kibana-status`: Process Kibana health and status information from `kibana_status.json`.
- `kibana-settings`: Process Kibana configuration and settings.
- `kibana-security`: Process Kibana security-related data (roles, users, actions, privileges).
- `kibana-fleet`: Process Fleet-specific data (agents, policies, packages, settings).
- `kibana-alerts`: Process Kibana alerts and alert health.
- `kibana-spaces`: Process Kibana spaces data.
- `kibana-task-manager`: Process Task Manager health.
- `kibana-stack-monitoring`: Process Stack Monitoring health.
- `kibana-synthetics-uptime`: Process Synthetics and Uptime settings and locations.
- `kibana-detection-engine`: Process Detection Engine health and rules.
- `kibana-metadata`: Extract and provide common metadata (diagnostic and node info) for document enrichment.
- `kibana-processing-runtime`: Discover scoped and paginated inputs, honor processing selections, report processing outcomes, and install compatible output templates.

### Modified Capabilities
<!-- None -->

## Impact

- `src/processor/mod.rs`: To register the new `kibana` module.
- `src/processor/kibana`: New directory for Kibana-specific logic.
- `assets/kibana/sources.yml`: Mark implemented sources as processable and define their processing dependencies.
- Elasticsearch index templates for every emitted Kibana metrics stream, with explicit dashboard metadata mappings and dynamic payload mappings.
