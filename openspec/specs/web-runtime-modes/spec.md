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
When runtime mode is `service`, the system SHALL enforce shared-instance behavior: authenticate requests from identity-aware-proxy headers, disable local credential persistence, skip reads and writes to `hosts.yml` and similar local artifacts, expose only limited user preferences, and use a single exporter defined at startup.

#### Scenario: Service mode request processing
- **GIVEN** the web server is running in `service` mode
- **WHEN** a user submits a web request that requires identity and export configuration
- **THEN** the system resolves identity from required proxy headers and processes the request using the startup-defined exporter
- **AND** the system does not read or write `hosts.yml` or other local persistent artifacts

### Requirement: User Mode Behavior Contract
When runtime mode is `user`, the system SHALL enforce single-user local behavior: no external auth requirement by default, allow saved credentials, permit reading and writing `hosts.yml` and related local artifacts, provide configurable user settings, and allow exporter changes at runtime.

#### Scenario: User mode settings and exporter updates
- **GIVEN** the web server is running in `user` mode
- **WHEN** the user updates host credentials and exporter preferences through the UI
- **THEN** the system persists allowed local artifacts and applies exporter changes to subsequent operations without restart

### Requirement: CLI Behavior Isolation
Runtime mode behavior SHALL apply only to the web interface and MUST NOT change CLI command behavior.

#### Scenario: CLI command remains unchanged
- **GIVEN** a user runs a CLI command outside web execution
- **WHEN** runtime mode features are present in the codebase
- **THEN** CLI execution semantics and outputs remain unchanged by `service` and `user` mode logic
