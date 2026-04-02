# web-runtime-modes (delta)

## MODIFIED Requirements

### Requirement: Service Mode Behavior Contract
When runtime mode is `service`, the system SHALL enforce shared-instance behavior: authenticate requests from identity-aware-proxy headers, disable local credential persistence, skip reads and writes to `hosts.yml`, `jobs.yml`, and similar local artifacts, expose only limited user preferences, use a single exporter defined at startup, and omit the user-mode workflow pages and saved-jobs UI entirely.

#### Scenario: Service mode request processing
- **GIVEN** the web server is running in `service` mode
- **WHEN** a user submits a web request that requires identity and export configuration
- **THEN** the system resolves identity from required proxy headers and processes the request using the startup-defined exporter
- **AND** the system does not read or write `hosts.yml`, `jobs.yml`, or other local persistent artifacts

#### Scenario: Jobs workflow omitted in service mode
- **GIVEN** the web server is running in `service` mode
- **WHEN** the user requests `/jobs` or `/workflow`
- **THEN** the service-mode router does not expose those user-mode pages

### Requirement: User Mode Behavior Contract
When runtime mode is `user`, the system SHALL enforce single-user local behavior: no external auth requirement by default, allow saved credentials, permit reading and writing `hosts.yml`, `jobs.yml`, and related local artifacts, provide configurable user settings, allow exporter changes at runtime, and expose the saved-jobs UI panel and Save button on the `/jobs` page.

#### Scenario: User mode settings and exporter updates
- **GIVEN** the web server is running in `user` mode
- **WHEN** the user updates host credentials and exporter preferences through the UI
- **THEN** the system persists allowed local artifacts and applies exporter changes to subsequent operations without restart

#### Scenario: Saved-jobs UI visible in user mode
- **GIVEN** the web server is running in `user` mode
- **WHEN** the user views the `/jobs` page
- **THEN** the saved-jobs left panel and Save button are rendered and functional
