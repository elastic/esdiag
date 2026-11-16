# kibana-fleet Specification

## Purpose
Process Fleet-specific Kibana diagnostic data, including agents, policies, packages, settings, and agent status, into ESDiag settings and metrics.

## Requirements

### Requirement: Kibana Fleet Processing
The system SHALL process Fleet-specific diagnostic data.

#### Scenario: Successful fleet data processing
- **GIVEN** a Kibana diagnostic bundle that contains Fleet policy or package files
- **WHEN** the bundle is processed
- **THEN** the system exports them to the `settings-kibana.fleet-esdiag` data stream

#### Scenario: Successful fleet agent status processing
- **GIVEN** a Kibana diagnostic bundle that contains `kibana_fleet_agent_status.json`
- **WHEN** the bundle is processed
- **THEN** the system exports it to the `metrics-kibana.fleet-esdiag` data stream
