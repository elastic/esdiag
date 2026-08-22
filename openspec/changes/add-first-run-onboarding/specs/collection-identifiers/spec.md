## ADDED Requirements

### Requirement: Persisted Default User Identifier
Collection and processing workflows SHALL resolve the diagnostic user identifier using explicit `--user`, then `ESDIAG_USER`, then `ApplicationConfig.user`. An absent value at every level SHALL remain absent, and persisted configuration MUST NOT override an explicit invocation or environment value.

#### Scenario: Persisted user supplies omitted identifier
- **GIVEN** `esdiag.yml` contains `user: reno@example.com`
- **AND** neither `--user` nor `ESDIAG_USER` is supplied
- **WHEN** a collection or processing workflow constructs `Identifiers`
- **THEN** its user identifier is `reno@example.com`

#### Scenario: Explicit user wins
- **GIVEN** `esdiag.yml` and `ESDIAG_USER` both contain default values
- **WHEN** the user invokes a workflow with `--user explicit@example.com`
- **THEN** the diagnostic user identifier is `explicit@example.com`

#### Scenario: Environment user overrides persistence
- **GIVEN** `esdiag.yml` contains a configured user
- **AND** `ESDIAG_USER` contains a different user
- **WHEN** a workflow omits `--user`
- **THEN** the diagnostic user identifier comes from `ESDIAG_USER`
