# kibana-node-stats Specification

## Purpose
Process Kibana node statistics, such as memory usage, event loop latency, and response times, from `kibana_stats.json` into ESDiag metrics.

## Requirements

### Requirement: Kibana Node Stats Processing
The system SHALL process Kibana node statistics from `kibana_stats.json`.

#### Scenario: Successful node stats processing
- **GIVEN** a Kibana diagnostic bundle that contains `kibana_stats.json`
- **WHEN** the bundle is processed
- **THEN** the system extracts metrics (process, os, response_times) and sends them to the `metrics-kibana.node-esdiag` data stream
