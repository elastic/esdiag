## ADDED Requirements

### Requirement: Web Exporter Resolution Precedence
Web processing SHALL use an exporter explicitly selected by the UI when one is present. The UI SHALL present the absence of an explicit exporter as `Default` and submit that selection as `null`, which the server SHALL deserialize as `None` and resolve through the existing `ESDIAG_OUTPUT_*` environment fallback. If neither an explicit UI exporter nor a valid environment output is available, processing SHALL fail instead of selecting stdout.

The runtime environment target SHALL NOT be rendered as a second explicit output option alongside `Default`; additional selectable remote outputs SHALL be saved hosts.

#### Scenario: Explicit UI output wins
- **GIVEN** the UI specifies an output target and runtime output environment variables are also present
- **WHEN** the job resolves its exporter
- **THEN** it uses the UI-selected output target

#### Scenario: Omitted UI output uses runtime output
- **GIVEN** the UI displays `Default` and submits its output signal as `null`
- **AND** valid runtime output environment variables identify an Elasticsearch target
- **WHEN** the job resolves its exporter
- **THEN** the server receives `None` for the explicit exporter
- **AND** uses an Elasticsearch exporter configured from the runtime environment

#### Scenario: Runtime target is represented only by Default
- **GIVEN** `ESDIAG_OUTPUT_URL` is `http://elasticsearch:9200`
- **AND** no saved remote output hosts exist
- **WHEN** the Advanced or Job Builder page renders its remote output selector
- **THEN** the selector contains `Default`
- **AND** does not contain a separate `http://elasticsearch:9200` option

#### Scenario: Missing or incomplete fallback fails closed
- **GIVEN** the UI does not specify an output target
- **AND** runtime output environment variables are missing or do not form a valid output target and authentication configuration
- **WHEN** the job resolves its exporter
- **THEN** processing fails with an actionable configuration error
- **AND** does not fall back to stdout

#### Scenario: Remote collection setup fails
- **WHEN** receiver or exporter setup fails after the user starts a remote processing job
- **THEN** the processing entry is replaced by a persistent failure entry in the job feed
- **AND** the loading and processing signals return to their terminal state
