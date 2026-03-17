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
