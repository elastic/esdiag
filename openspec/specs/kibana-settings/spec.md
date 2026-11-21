# kibana-settings Specification

## Purpose
Process Kibana configuration and settings files, such as Fleet and Uptime settings, into flattened ESDiag settings documents.

## Requirements

### Requirement: Kibana Settings Processing
The system SHALL process Kibana configuration and settings.

#### Scenario: Successful settings processing
- **GIVEN** a Kibana diagnostic bundle that contains settings files like `kibana_fleet_settings.json` or `kibana_uptime_settings.json`
- **WHEN** the bundle is processed
- **THEN** the system flattens and sends them to the `settings-kibana.fleet-esdiag` or `settings-kibana.uptime-esdiag` data streams respectively
