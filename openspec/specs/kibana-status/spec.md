# kibana-status Specification

## Purpose
Process Kibana health and status information, including overall health and plugin states, from `kibana_status.json` into ESDiag metrics.

## Requirements

### Requirement: Kibana Status Processing
The system SHALL process Kibana health and status from `kibana_status.json`.

#### Scenario: Successful status processing
- **GIVEN** a Kibana diagnostic bundle that contains `kibana_status.json`
- **WHEN** the bundle is processed
- **THEN** the system extracts overall health and plugin states, sending them to the `metrics-kibana.status-esdiag` data stream
