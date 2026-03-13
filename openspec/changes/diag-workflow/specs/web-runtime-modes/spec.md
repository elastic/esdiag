## ADDED Requirements

### Requirement: Mode-Aware Remote Collection Inputs
The `Collect` panel SHALL adapt its `Collect -> Collect` inputs to the active web runtime mode. In `user` mode, remote collection SHALL allow selecting from saved known hosts. In `service` mode, remote collection SHALL require explicit endpoint and API key inputs instead of local known-host selection.

#### Scenario: User mode remote collection uses saved host
- **GIVEN** the web interface is running in `user` mode
- **WHEN** the user selects `Collect -> Collect` in the `Collect` panel
- **THEN** the UI offers saved known hosts as selectable remote collection sources

#### Scenario: Service mode remote collection uses explicit credentials
- **GIVEN** the web interface is running in `service` mode
- **WHEN** the user selects `Collect -> Collect` in the `Collect` panel
- **THEN** the UI requires explicit endpoint and API key inputs
- **AND** the workflow does not depend on local known-host artifacts for the remote source

### Requirement: Mode-Aware Bundle Persistence
The workflow SHALL allow local bundle persistence only when the active runtime mode permits local artifacts.

#### Scenario: Service mode rejects local bundle save behavior
- **GIVEN** the web interface is running in `service` mode
- **WHEN** the user configures `Collect -> Collect`
- **THEN** the workflow does not expose or honor local bundle save behavior that depends on local persisted artifacts

#### Scenario: User mode defaults bundle save path locally
- **GIVEN** the web interface is running in `user` mode
- **WHEN** the user enables `Save Bundle`
- **THEN** the workflow may suggest the current user's operating-system-aware `Downloads` directory as the default local save path
- **AND** the user may change that path before execution
