## ADDED Requirements

### Requirement: Kibana Synthetics and Uptime Processing
The system SHALL process Synthetics and Uptime settings and locations.

#### Scenario: Successful synthetics and uptime processing
- **WHEN** configuration files for synthetics or uptime are processed
- **THEN** the system exports them to the `settings-kibana.synthetics-esdiag` or `settings-kibana.uptime-esdiag` data streams respectively
