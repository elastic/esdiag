## ADDED Requirements

### Requirement: Send Host Selection Filtering
When the `Send` panel offers known-host delivery for processed diagnostics, the system SHALL list only Elasticsearch known hosts that are valid for the `send` role. For `Send -> Local`, known-host delivery SHALL be further restricted to localhost-style targets.

#### Scenario: User selects a processed diagnostic target
- **GIVEN** the workflow has a processed diagnostic ready to send
- **AND** the known host inventory contains hosts with mixed roles
- **WHEN** the `Send` panel displays known-host target options
- **THEN** only Elasticsearch hosts with the `send` role are presented as selectable send targets
- **AND** hosts without the `send` role are excluded from the list

#### Scenario: Known-host target is disabled by incompatible workflow state
- **GIVEN** an Elasticsearch known host is valid for the `send` role
- **AND** the current workflow is configured for archive delivery without processing
- **WHEN** the `Send` panel displays target options
- **THEN** the processed-output known host target is disabled because the workflow state is incompatible

#### Scenario: Local known-host target requires localhost
- **GIVEN** the workflow is configured for processed local delivery
- **WHEN** the `Send` panel displays known-host target options
- **THEN** only `send`-role Elasticsearch hosts whose address resolves to `localhost` or `127.0.0.1` are valid local known-host targets
