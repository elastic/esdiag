## ADDED Requirements

### Requirement: Kibana Security Processing
The system SHALL process Kibana security-related data including roles, users, and actions.

#### Scenario: Successful security data processing
- **WHEN** security files like `kibana_roles.json`, `kibana_user.json`, or `kibana_actions.json` are processed
- **THEN** the system exports them to the `settings-kibana.security-esdiag` data stream
