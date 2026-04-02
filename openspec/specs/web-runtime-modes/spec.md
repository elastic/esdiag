# web-runtime-modes

## Purpose

Defines explicit runtime modes (`service` and `user`) for the web interface, governing authentication, credential persistence, host configuration, and exporter behavior across both `serve` and desktop-hosted variants.

## ADDED Requirements

### Requirement: Web Runtime Mode Declaration
The web interface SHALL run in an explicit runtime mode of `service` or `user` for both `serve` and desktop-hosted variants. Mode resolution MUST follow this precedence order: explicit `--mode` argument, then `ESDIAG_MODE` environment variable, then the runtime default.

#### Scenario: Startup resolves runtime mode
- **GIVEN** the web server is starting through `serve` or a desktop wrapper
- **WHEN** startup configuration is loaded
- **THEN** the server state contains exactly one runtime mode value (`service` or `user`) used by web handlers

#### Scenario: CLI mode overrides environment mode
- **GIVEN** `ESDIAG_MODE=service` is set in the process environment
- **WHEN** the server starts with `--mode user`
- **THEN** the effective runtime mode is `user`

#### Scenario: Environment mode is used when CLI mode is absent
- **GIVEN** `ESDIAG_MODE=service` is set in the process environment
- **WHEN** the server starts without a `--mode` argument
- **THEN** the effective runtime mode is `service`

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

### Requirement: CLI Behavior Isolation
Runtime mode behavior SHALL apply only to the web interface and MUST NOT change CLI command behavior.

#### Scenario: CLI command remains unchanged
- **GIVEN** a user runs a CLI command outside web execution
- **WHEN** runtime mode features are present in the codebase
- **THEN** CLI execution semantics and outputs remain unchanged by `service` and `user` mode logic

### Requirement: Mode-Aware Remote Collection Inputs
The staged workflow routes SHALL be mounted only in `user` mode for now. Within that user-mode workflow, `Collect -> Collect` SHALL allow selecting from saved known hosts.

#### Scenario: User mode remote collection uses saved host
- **GIVEN** the web interface is running in `user` mode
- **WHEN** the user selects `Collect -> Collect` in the `Collect` panel
- **THEN** the UI offers saved known hosts as selectable remote collection sources

#### Scenario: Service mode does not mount advanced workflow routes
- **GIVEN** the web interface is running in `service` mode
- **WHEN** the user requests `/workflow` or `/jobs`
- **THEN** the server does not mount those routes
- **AND** advanced workflow configuration is deferred until a future design pass

### Requirement: Mode-Aware Bundle Persistence
The user-mode staged workflow SHALL support browser-managed bundle downloads without requiring a user-configured local filesystem save path.

#### Scenario: User mode exposes browser download save behavior
- **GIVEN** the web interface is running in `user` mode
- **WHEN** the user enables `Save Bundle`
- **THEN** the workflow uses browser-managed download behavior
- **AND** the workflow does not require manual local path entry before execution
