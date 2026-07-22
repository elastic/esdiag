## ADDED Requirements

### Requirement: Elastic CLI Extension Entrypoint
The system SHALL provide an Elastic CLI extension entrypoint named `elastic-diag` so the Elastic CLI can register and invoke ESDiag as the short extension command `diag`.

#### Scenario: Extension entrypoint forwards command arguments
- **WHEN** the Elastic CLI invokes the registered `elastic-diag` entrypoint with arguments `process input.zip`
- **THEN** the entrypoint delegates to the ESDiag execution layer with arguments equivalent to `esdiag process input.zip`
- **AND** the child command exit status is returned to the Elastic CLI caller

#### Scenario: Extension entrypoint marks Elastic CLI invocation
- **WHEN** the Elastic CLI invokes the registered `elastic-diag` entrypoint
- **THEN** the entrypoint sets `ESDIAG_ELASTIC_CLI=1` for the delegated ESDiag process
- **AND** ESDiag can treat that variable as the reliable marker that the invocation came through the Elastic CLI extension wrapper

#### Scenario: Existing standalone binary remains available
- **WHEN** a user invokes `esdiag process input.zip`
- **THEN** the standalone command path remains supported
- **AND** the command behavior is not changed only because the extension entrypoint exists

#### Scenario: Missing esdiag binary reports install guidance
- **GIVEN** the `elastic-diag` entrypoint is invoked
- **AND** no `esdiag` executable is available on `PATH`
- **WHEN** the wrapper attempts to delegate to ESDiag
- **THEN** the wrapper fails with a clear error explaining that `esdiag` must be installed
- **AND** the error includes current Cargo-based installation guidance for this repository

### Requirement: Elastic CLI Context Environment Fallback
The system SHALL accept Elastic CLI context environment variables as fallbacks for env-backed Elasticsearch, Kibana, and Cloud runtime configuration. `ESDIAG_*` variables MUST take precedence over `ELASTIC_*` variables when both are present.

#### Scenario: Elasticsearch output resolves from Elastic CLI API key context
- **GIVEN** `ESDIAG_OUTPUT_URL` is not set
- **AND** `ELASTIC_ES_URL` is set
- **AND** `ELASTIC_ES_API_KEY` is set
- **WHEN** a command resolves an omitted Elasticsearch output target
- **THEN** the system uses `ELASTIC_ES_URL` as the Elasticsearch URL
- **AND** the system authenticates using `ELASTIC_ES_API_KEY`

#### Scenario: Elasticsearch output preserves ESDiag precedence
- **GIVEN** `ESDIAG_OUTPUT_URL` is set
- **AND** `ELASTIC_ES_URL` is set to a different value
- **WHEN** a command resolves an omitted Elasticsearch output target
- **THEN** the system uses `ESDIAG_OUTPUT_URL`
- **AND** the Elastic CLI URL fallback is ignored for that resolution

#### Scenario: Kibana URL resolves from Elastic CLI context
- **GIVEN** `ESDIAG_KIBANA_URL` is not set
- **AND** `ELASTIC_KIBANA_URL` is set
- **WHEN** a command resolves Kibana context for setup, serving, or generated links
- **THEN** the system uses `ELASTIC_KIBANA_URL` as the Kibana URL

#### Scenario: Basic authentication resolves from Elastic CLI context
- **GIVEN** no ESDiag output authentication variables are set
- **AND** `ELASTIC_ES_USERNAME` is set
- **AND** `ELASTIC_ES_PASSWORD` is set
- **WHEN** a command resolves an omitted Elasticsearch output target
- **THEN** the system authenticates using the Elastic CLI username and password values

#### Scenario: Cloud API key resolves from Elastic CLI context
- **GIVEN** no ESDiag Cloud target variables are set
- **AND** `ELASTIC_CLOUD_URL` is set
- **AND** `ELASTIC_CLOUD_API_KEY` is set
- **WHEN** a command resolves an Elastic Cloud target
- **THEN** the system uses the Elastic CLI Cloud URL and API key through ESDiag's existing Cloud target path

### Requirement: Active Elastic Context Target References
The system SHALL support active-context leading-dot Elastic target references for command arguments that can resolve remote targets. A reference of `.service` SHALL resolve the service from the active Elastic CLI context passed through the extension environment. The service segment MUST identify a known service name or alias.

The service aliases MUST resolve as follows:
- `es` resolves to `elasticsearch`
- `kb` resolves to `kibana`
- `cloud` resolves to `cloud`

#### Scenario: Collect resolves active Elasticsearch context alias
- **GIVEN** the Elastic CLI extension wrapper has set `ESDIAG_ELASTIC_CLI=1`
- **AND** the active Elastic CLI context provides an Elasticsearch service
- **WHEN** the user runs `elastic diag collect .es ./out`
- **THEN** the collect source resolves to the active context's Elasticsearch service
- **AND** the output argument resolves to `./out`

#### Scenario: Resolver supports active Kibana alias
- **GIVEN** the active Elastic CLI context provides a Kibana service
- **WHEN** a command resolves `.kb`
- **THEN** the target service is interpreted as `kibana`

#### Scenario: Resolver supports active Cloud service
- **GIVEN** the active Elastic CLI context provides a Cloud service
- **WHEN** a command resolves `.cloud`
- **THEN** the target service is interpreted as `cloud`
- **AND** ESDiag uses the existing Cloud API key target path for that transient target

#### Scenario: Non-service leading-dot argument falls through
- **GIVEN** a command argument starts with `.`
- **AND** the rightmost segment is not a known service name or alias
- **WHEN** the command resolves that argument
- **THEN** the system does not treat it as an Elastic context target reference
- **AND** the argument continues through saved-host, URL, local file, directory, and stream resolution

#### Scenario: Hidden local file can bypass context target syntax
- **GIVEN** a local hidden file path would otherwise look like a context target reference
- **WHEN** the user provides the path with an explicit filesystem prefix such as `./.es`
- **THEN** the system resolves the argument through local filesystem handling instead of Elastic context target handling

### Requirement: Extension-Compatible Installation Metadata
The system SHALL include extension installation metadata or files that allow the Elastic CLI installer to discover an executable entrypoint for the `diag` extension.

#### Scenario: GitHub extension install discovers entrypoint
- **WHEN** the Elastic CLI installs the extension from a GitHub source named `elastic-diag`
- **THEN** the cloned extension contents expose an executable entrypoint named `elastic-diag` through an installer-supported location or package metadata
- **AND** the registered extension name is `diag`

### Requirement: Extension Documentation
The system SHALL document how to install and use ESDiag through the Elastic CLI extension system, including the relationship between `elastic diag` and `esdiag`.

#### Scenario: User reads extension installation docs
- **WHEN** a user reads the command-line documentation
- **THEN** the documentation includes an Elastic CLI extension installation example
- **AND** the documentation states that `elastic diag <args...>` delegates to the ESDiag CLI command surface
- **AND** the documentation lists the supported Elastic CLI context environment variables consumed by ESDiag

### Requirement: Elastic CLI Context-Aware Help
When `ESDIAG_ELASTIC_CLI=1` is present, ESDiag help output MAY include Elastic CLI-specific usage guidance. Context-aware help SHALL NOT be required for shell completion behavior.

#### Scenario: Help includes Elastic CLI examples under extension invocation
- **GIVEN** `ESDIAG_ELASTIC_CLI=1` is set
- **WHEN** the user runs `elastic diag --help`
- **THEN** help output includes Elastic CLI-specific examples or target reference guidance

#### Scenario: Standalone help remains focused on esdiag
- **GIVEN** `ESDIAG_ELASTIC_CLI` is not set
- **WHEN** the user runs `esdiag --help`
- **THEN** help output remains focused on standalone ESDiag usage
