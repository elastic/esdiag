## Purpose

Define host role assignment and validation for collect/send/view target selection.

## ADDED Requirements

### Requirement: Host Role Assignment
The system SHALL support host role assignments for `collect`, `send`, and `view` in host configuration. If roles are omitted, the system SHALL default the host role set to `collect`.

#### Scenario: Roles omitted in host configuration
- **GIVEN** a host entry has no explicit `roles` field
- **WHEN** the system validates host configuration
- **THEN** the host is assigned the `collect` role by default

### Requirement: Role and Host Type Validation
The system SHALL enforce host-type constraints for role assignment where `send` is valid only on Elasticsearch hosts and `view` is valid only on Kibana hosts.

#### Scenario: Invalid send role on non-Elasticsearch host
- **GIVEN** a Kibana host entry includes role `send`
- **WHEN** the system validates host configuration
- **THEN** validation fails with an error indicating `send` is only valid for Elasticsearch hosts

#### Scenario: Invalid view role on non-Kibana host
- **GIVEN** an Elasticsearch host entry includes role `view`
- **WHEN** the system validates host configuration
- **THEN** validation fails with an error indicating `view` is only valid for Kibana hosts

### Requirement: Role-Based Target Filtering
The system SHALL provide role-based host filtering outputs for runtime workflows so collect, send, and view phases can select only hosts matching each phase role.

#### Scenario: Build collect target list
- **GIVEN** a host inventory containing mixed role assignments
- **WHEN** the system resolves targets for the collect phase
- **THEN** only hosts with role `collect` are included in collect targets

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
