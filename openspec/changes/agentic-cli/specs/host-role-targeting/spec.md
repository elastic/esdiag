## ADDED Requirements

### Requirement: Viewer Reference Resolution For Send Hosts
When a saved host with role `send` includes a `viewer` reference, the system SHALL resolve that reference to the corresponding saved host with role `view` so downstream processing and reporting can use that viewer host as the canonical Kibana target.

#### Scenario: Send host resolves its saved viewer host
- **GIVEN** a saved Elasticsearch host includes role `send` and `viewer: prod-kb`
- **AND** `prod-kb` is a saved Kibana host with role `view`
- **WHEN** the system resolves the send host's viewer target for processed diagnostic reporting
- **THEN** the resolved viewer target is the saved `prod-kb` host

#### Scenario: Send host without viewer has no resolved viewer target
- **GIVEN** a saved Elasticsearch host includes role `send`
- **AND** the host does not define a `viewer` reference
- **WHEN** the system resolves the send host's viewer target for processed diagnostic reporting
- **THEN** no saved viewer target is resolved
