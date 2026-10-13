## ADDED Requirements

### Requirement: Kibana Status Processing
The system SHALL process Kibana health and status from `kibana_status.json`.

#### Scenario: Successful status processing
- **WHEN** `kibana_status.json` is provided in the diagnostic bundle
- **THEN** the system extracts overall health and plugin states, sending them to the `metrics-kibana.status-esdiag` data stream
