# kibana-security Specification

## Purpose
Process Kibana security-related diagnostic data, including roles, users, actions, and privileges, into ESDiag settings documents.

## Requirements

### Requirement: Kibana Security Processing
The system SHALL process Kibana security-related data including roles, users, and actions.

#### Scenario: Successful security data processing
- **GIVEN** a Kibana diagnostic bundle that contains security files like `kibana_roles.json`, `kibana_user.json`, or `kibana_actions.json`
- **WHEN** the bundle is processed
- **THEN** the system exports them to the `settings-kibana.security-esdiag` data stream
