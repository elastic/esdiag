## ADDED Requirements

### Requirement: Kibana Node Stats Processing
The system SHALL process Kibana node statistics from `kibana_stats.json`.

#### Scenario: Successful node stats processing
- **WHEN** `kibana_stats.json` is provided in the diagnostic bundle
- **THEN** the system extracts metrics (process, os, response_times) and sends them to the `metrics-kibana.node-esdiag` data stream
