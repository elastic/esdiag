## ADDED Requirements

### Requirement: Serve Exporter Resolution Precedence
Web `serve` startup SHALL resolve its initial exporter in this order: an explicit output argument, a complete runtime output configuration, then the mode-specific fallback. Runtime output configuration SHALL be considered requested when any `ESDIAG_OUTPUT_URL`, `ESDIAG_OUTPUT_APIKEY`, `ESDIAG_OUTPUT_USERNAME`, or `ESDIAG_OUTPUT_PASSWORD` variable is present.

#### Scenario: Explicit output wins
- **GIVEN** an explicit output argument and runtime output environment variables are both present
- **WHEN** `serve` resolves its initial exporter
- **THEN** it uses the explicit output argument

#### Scenario: User mode uses runtime output
- **GIVEN** `serve` runs in User mode without an explicit output argument
- **AND** valid runtime output environment variables identify an Elasticsearch target
- **WHEN** the initial exporter is resolved
- **THEN** the server uses an Elasticsearch exporter configured from the runtime environment
- **AND** retains User-mode local features and exporter controls

#### Scenario: Unconfigured User mode uses stdout
- **GIVEN** `serve` runs in User mode without an explicit output argument
- **AND** none of the runtime output environment variables are present
- **WHEN** the initial exporter is resolved
- **THEN** the server uses the stdout exporter

#### Scenario: Incomplete runtime output fails closed
- **GIVEN** one or more runtime output environment variables are present
- **AND** they do not form a valid output target and authentication configuration
- **WHEN** `serve` resolves its initial exporter
- **THEN** startup fails with an actionable configuration error
- **AND** does not fall back to stdout

#### Scenario: Service mode requires configured output
- **GIVEN** `serve` runs in Service mode without an explicit output argument
- **WHEN** no valid runtime output configuration is available
- **THEN** startup fails without selecting stdout

### Requirement: Exporter Credential Origin
The web server SHALL retain whether the active exporter's credentials came from runtime configuration or local keystore-backed state. Keystore decisions MUST use credential origin rather than infer ownership solely from an exporter URL matching a saved host.

#### Scenario: Runtime exporter matches saved secure host URL
- **GIVEN** the initial exporter is fully authenticated from runtime environment variables
- **AND** a saved keystore-backed host has the same Elasticsearch URL
- **WHEN** processing preflight evaluates the active exporter
- **THEN** the exporter remains classified as runtime-backed
- **AND** the matching URL does not create a keystore dependency

#### Scenario: User selects a saved secure host
- **GIVEN** the server is running with a runtime-backed exporter
- **WHEN** the user selects a saved keystore-backed output host
- **THEN** the active exporter origin changes to local keystore-backed state
- **AND** subsequent processing applies the keystore unlock policy
