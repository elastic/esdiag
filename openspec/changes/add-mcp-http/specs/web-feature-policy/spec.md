## MODIFIED Requirements

### Requirement: Web Feature Allowlist
The server SHALL support `serve --web-features` and `ESDIAG_WEB_FEATURES` as optional comma-separated allowlists of web feature names. Supported feature names SHALL include `advanced`, `job-builder`, and `mcp`. When neither override is set, the server SHALL use runtime-mode defaults, and `mcp` SHALL be disabled by default in both runtime modes. When an override is set, the server SHALL enable exactly the listed known web features after trimming whitespace. When an override is set to an empty or whitespace-only value, the server SHALL disable all optional web features. The CLI argument SHALL take precedence over the environment variable.

#### Scenario: Unset feature list uses user defaults
- **GIVEN** the server starts in `user` mode
- **AND** `ESDIAG_WEB_FEATURES` is unset
- **AND** `--web-features` is not provided
- **WHEN** server policy is constructed
- **THEN** the `advanced` web feature is enabled
- **AND** the `job-builder` and `mcp` web features are disabled

#### Scenario: Desktop startup uses user defaults
- **GIVEN** the desktop-hosted server starts in `user` mode
- **AND** `ESDIAG_WEB_FEATURES` is unset
- **WHEN** server policy is constructed
- **THEN** the `advanced` web feature is enabled
- **AND** the `job-builder` and `mcp` web features are disabled

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

#### Scenario: Service mode enables MCP explicitly
- **GIVEN** the server starts in `service` mode
- **AND** `--web-features mcp` is provided
- **WHEN** server policy is constructed
- **THEN** the `mcp` web feature is enabled

#### Scenario: Unknown feature name fails startup
- **GIVEN** `ESDIAG_WEB_FEATURES=advanced,unknown-feature` is set
- **WHEN** the server starts
- **THEN** startup fails with an error naming `unknown-feature`
- **AND** the error lists `advanced`, `job-builder`, and `mcp` as supported feature names

#### Scenario: Unsupported build rejects job builder
- **GIVEN** the server binary was built without `keystore` support
- **AND** `ESDIAG_WEB_FEATURES=job-builder` is set or `--web-features job-builder` is provided
- **WHEN** the server starts
- **THEN** startup fails with an error naming `job-builder`
- **AND** the error explains that Job Builder requires a build with keystore support

#### Scenario: Unsupported build rejects MCP
- **GIVEN** the server binary was built without `mcp` support
- **AND** `ESDIAG_WEB_FEATURES=mcp` is set or `--web-features mcp` is provided
- **WHEN** the server starts
- **THEN** startup fails with an error naming `mcp`
- **AND** the error explains that the MCP endpoint requires a build with MCP support
