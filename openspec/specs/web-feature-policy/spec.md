# web-feature-policy

## Purpose

Defines runtime web feature allowlists and the unified server policy used to compose web feature availability with server runtime mode.

## ADDED Requirements

### Requirement: Web Feature Allowlist
The server SHALL support `serve --web-features` and `ESDIAG_WEB_FEATURES` as optional comma-separated allowlists of web feature names. Supported feature names SHALL include `advanced` and `job-builder`. When neither override is set, the server SHALL use runtime-mode defaults. When an override is set, the server SHALL enable exactly the listed known web features after trimming whitespace. When an override is set to an empty or whitespace-only value, the server SHALL disable all optional web features. The CLI argument SHALL take precedence over the environment variable.

#### Scenario: Unset feature list uses user defaults
- **GIVEN** the server starts in `user` mode
- **AND** `ESDIAG_WEB_FEATURES` is unset
- **AND** `--web-features` is not provided
- **WHEN** server policy is constructed
- **THEN** the `advanced` web feature is enabled
- **AND** the `job-builder` web feature is disabled

#### Scenario: Desktop startup uses user defaults
- **GIVEN** the desktop-hosted server starts in `user` mode
- **AND** `ESDIAG_WEB_FEATURES` is unset
- **WHEN** server policy is constructed
- **THEN** the `advanced` web feature is enabled
- **AND** the `job-builder` web feature is disabled

#### Scenario: Unset feature list uses service defaults
- **GIVEN** the server starts in `service` mode
- **AND** `ESDIAG_WEB_FEATURES` is unset
- **AND** `--web-features` is not provided
- **WHEN** server policy is constructed
- **THEN** no optional web features are enabled

#### Scenario: Environment feature list is authoritative
- **GIVEN** the server starts in `user` mode
- **AND** `ESDIAG_WEB_FEATURES=job-builder` is set
- **AND** `--web-features` is not provided
- **WHEN** server policy is constructed
- **THEN** the `job-builder` web feature is enabled
- **AND** the `advanced` web feature is disabled

#### Scenario: CLI feature list overrides environment
- **GIVEN** the server starts in `user` mode
- **AND** `ESDIAG_WEB_FEATURES=advanced,job-builder` is set
- **AND** `--web-features advanced` is provided
- **WHEN** server policy is constructed
- **THEN** the `advanced` web feature is enabled
- **AND** the `job-builder` web feature is disabled

#### Scenario: Empty feature list disables optional web features
- **GIVEN** the server starts in `user` mode
- **AND** `ESDIAG_WEB_FEATURES` is set to an empty string
- **AND** `--web-features` is not provided
- **WHEN** server policy is constructed
- **THEN** no optional web features are enabled

#### Scenario: Unknown feature name fails startup
- **GIVEN** `ESDIAG_WEB_FEATURES=advanced,unknown-feature` is set
- **WHEN** the server starts
- **THEN** startup fails with an error naming `unknown-feature`
- **AND** the error lists `advanced` and `job-builder` as supported feature names

### Requirement: Unified Server Policy Decisions
The server SHALL expose a single `ServerPolicy` decision surface that composes runtime mode and web feature availability. Route registration, web handlers, and templates SHALL use policy decision methods rather than independently combining raw runtime mode and environment variable checks.

#### Scenario: Policy gates Advanced route
- **GIVEN** server policy allows the `advanced` web feature
- **WHEN** routes are registered
- **THEN** the `/advanced` route is mounted
- **AND** the `/workflow` route is not mounted

#### Scenario: Policy omits disabled route
- **GIVEN** server policy does not allow the `job-builder` web feature
- **WHEN** routes are registered
- **THEN** the `/jobs` route is not mounted

#### Scenario: Service mode remains safety envelope
- **GIVEN** the server starts in `service` mode
- **AND** `ESDIAG_WEB_FEATURES=advanced,job-builder` is set
- **WHEN** routes are registered
- **THEN** local-artifact-backed web routes are not mounted
- **AND** service-mode authentication and exporter restrictions remain enforced
