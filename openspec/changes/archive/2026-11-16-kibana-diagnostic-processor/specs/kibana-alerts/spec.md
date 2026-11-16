## ADDED Requirements

### Requirement: Kibana Alerts Processing
The system SHALL process Kibana alerts and alert health data.

#### Scenario: Successful alerts processing
- **WHEN** `kibana_alerts_1.json` is processed
- **THEN** the system exports it to the `settings-kibana.alerts-esdiag` data stream

#### Scenario: Successful alert health processing
- **WHEN** `kibana_alerts_health.json` is processed
- **THEN** the system exports it to the `metrics-kibana.alerts-esdiag` data stream
