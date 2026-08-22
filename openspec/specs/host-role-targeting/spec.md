## Purpose

Define host role assignment and validation for collect/send/view target selection.

## Requirements

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

### Requirement: Application-Based Host Role Validation
The system SHALL validate application-specific host roles against the resolved target
`Application`, independent of whether the endpoint uses a direct or Cloud admin route.
The `send` role MUST resolve to Elasticsearch and the `view` role MUST resolve to
Kibana.

#### Scenario: Cloud admin Elasticsearch target accepts send role
- **GIVEN** a concrete host reaches Elasticsearch through a Cloud admin route
- **AND** the host has the `send` role
- **WHEN** the system validates the host
- **THEN** validation succeeds because the resolved application is Elasticsearch
- **AND** the route is not treated as the application

#### Scenario: Direct Kibana target accepts view role
- **GIVEN** a concrete direct host resolves to Kibana
- **AND** the host has the `view` role
- **WHEN** the system validates the host
- **THEN** validation succeeds

#### Scenario: Platform value cannot satisfy role validation
- **GIVEN** a legacy host contains a platform value where an application is required
- **WHEN** the system validates a `send` or `view` role
- **THEN** the platform value does not satisfy the role constraint
- **AND** the system requires the host to resolve to a compatible application

### Requirement: Deferred Dynamic-Template Role Validation
The system SHALL validate role constraints immediately when a template fixes its
application. When a dynamic template defers application selection to materialization,
the system SHALL validate the saved role set against each selected application before
returning a resolved host.

#### Scenario: Materialized send template selects Elasticsearch
- **GIVEN** a dynamic template has the `send` role
- **WHEN** a reference materializes it with application `Elasticsearch`
- **THEN** role validation succeeds

#### Scenario: Materialized send template selects Kibana
- **GIVEN** a dynamic template has the `send` role
- **WHEN** a reference attempts to materialize it with application `Kibana`
- **THEN** resolution fails because `send` is valid only for Elasticsearch

#### Scenario: Fixed Kibana template rejects send role when saved
- **GIVEN** a template is fixed to application `Kibana`
- **WHEN** the user attempts to save it with the `send` role
- **THEN** validation fails without waiting for materialization
