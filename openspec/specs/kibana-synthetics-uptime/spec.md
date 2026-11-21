# kibana-synthetics-uptime Specification

## Purpose
Process Kibana Synthetics and Uptime settings and locations from Kibana diagnostic bundles into ESDiag settings documents.

## Requirements

### Requirement: Kibana Synthetics and Uptime Processing
The system SHALL process Synthetics and Uptime settings and locations.

#### Scenario: Successful synthetics and uptime processing
- **GIVEN** a Kibana diagnostic bundle that contains configuration files for synthetics or uptime
- **WHEN** the bundle is processed
- **THEN** the system exports them to the `settings-kibana.synthetics-esdiag` or `settings-kibana.uptime-esdiag` data streams respectively
