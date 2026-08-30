## ADDED Requirements

### Requirement: Elastic CLI Extension Entrypoint
The shared ESDiag Rust execution layer SHALL support an Elastic CLI invocation profile named `elastic-diag` so the Elastic CLI can invoke it as the intentionally shortened command `elastic diag`. Distribution and self-contained installation of the native binary are specified separately.

#### Scenario: Native entrypoint uses the shared execution layer
- **WHEN** the Elastic CLI invokes an `elastic-diag` entrypoint
- **THEN** the executable uses the same Job builder, executor, receivers, processors, and exporters as `esdiag`
- **AND** the command exit status is returned to the Elastic CLI caller

#### Scenario: Executable name selects the extension profile
- **WHEN** the shared binary is invoked as `elastic-diag` or `elastic-diag.exe`
- **THEN** it selects the Elastic CLI extension command profile
- **AND** generated usage text identifies the command as `elastic diag`
- **AND** `ESDIAG_ELASTIC_CLI=1` MAY remain as a development or compatibility override but is not the primary identity mechanism

#### Scenario: Existing standalone binary remains available
- **WHEN** a user invokes `esdiag process input.zip`
- **THEN** the standalone command path remains supported
- **AND** the command behavior is not changed only because the extension entrypoint exists

### Requirement: Extension-Specific Command Profile
The `elastic diag` surface SHALL expose only commands appropriate to context-backed diagnostic execution. The initial profile SHALL expose `collect`, `process`, `send`, `setup`, `job`, `output`, `help`, and `version`. It SHALL NOT expose standalone deployment or credential-administration commands including `local`, `serve`, `init`, `host`, `keystore`, or `agent`.

#### Scenario: Outbound bundle transfer uses Send vocabulary
- **WHEN** a user transmits an existing bundle to Elastic Upload Service through the extension
- **THEN** the command is `elastic diag send`
- **AND** the operation maps to the Send stage
- **AND** the extension consistently describes that outbound operation as Send

#### Scenario: Standalone administration remains available
- **WHEN** a user needs to manage saved hosts, the keystore, local deployments, or the web server
- **THEN** those commands remain available through the standalone `esdiag` binary
- **AND** they are omitted from `elastic diag` help

### Requirement: Active Elastic CLI Context Input
The extension SHALL consume resolved Elasticsearch and Kibana service values passed by Elastic CLI as runtime-only input targets. It SHALL keep Elasticsearch and Kibana authentication separate and SHALL NOT persist Elastic CLI credentials to ESDiag saved hosts or the keystore.

#### Scenario: Elasticsearch input resolves from active context
- **GIVEN** `ELASTIC_ES_URL` is set
- **AND** `ELASTIC_ES_API_KEY` is set
- **WHEN** a command resolves `.es`
- **THEN** the system constructs a transient Elasticsearch Collect target from `ELASTIC_ES_URL`
- **AND** the system authenticates using `ELASTIC_ES_API_KEY`

#### Scenario: Context credentials do not become generic output fallbacks
- **GIVEN** the active context identifies a frequently changing input deployment
- **WHEN** the extension resolves an omitted output
- **THEN** it does not implicitly export processed documents back to the active input context
- **AND** output resolution uses the explicit or configured output deployment rules

### Requirement: Active Elastic Context Target References
The system SHALL support active-context leading-dot Elastic target references for command arguments that can resolve remote targets. A reference of `.service` SHALL resolve the service from the active Elastic CLI context passed through the extension environment. The service segment MUST identify a known service name or alias.

The service aliases MUST resolve as follows:
- `es` resolves to `elasticsearch`
- `kb` resolves to `kibana`

#### Scenario: Collect resolves active Elasticsearch context alias
- **GIVEN** the shared binary is running under the Elastic CLI extension profile
- **AND** the active Elastic CLI context provides an Elasticsearch service
- **WHEN** the user runs `elastic diag collect .es ./out`
- **THEN** the collect source resolves to the active context's Elasticsearch service
- **AND** the output argument resolves to `./out`

#### Scenario: Resolver supports active Kibana alias
- **GIVEN** the active Elastic CLI context provides a Kibana service
- **WHEN** a command resolves `.kb`
- **THEN** the target service is interpreted as `kibana`

#### Scenario: Application is never guessed
- **GIVEN** the active Elastic CLI context contains more than one application service
- **AND** a named output context has been configured
- **WHEN** the user runs `elastic diag process` without an input
- **THEN** the command fails before starting a Job
- **AND** the error instructs the user to select `.es`, `.kb`, or an explicit named-context target

#### Scenario: Process uses selected active application and configured output
- **GIVEN** Elastic CLI's active context is `prod`
- **AND** ESDiag's configured output context is `monitoring`
- **WHEN** the user runs `elastic diag process .es`
- **THEN** the Job Collects from the active context's Elasticsearch application
- **AND** Processes the collected diagnostic
- **AND** Exports the processed documents to the `monitoring` output deployment

### Requirement: Configured Elastic CLI Output Context
The extension SHALL allow a relatively fixed named Elastic CLI context to be stored as the default output deployment reference with `elastic diag output set <context>`. The stored value SHALL be a symbolic context reference, not a URL or credential. `elastic diag output show` and `elastic diag output clear` SHALL inspect and remove that reference without mutating `.elasticrc`.

#### Scenario: Configure named output context
- **GIVEN** `.elasticrc` contains context `monitoring` with an Elasticsearch service
- **WHEN** the user runs `elastic diag output set monitoring`
- **THEN** ESDiag validates the context as an output deployment
- **AND** persists only the config source and context name in ESDiag application configuration
- **AND** does not copy credentials into ESDiag storage

#### Scenario: Output context resolves as one deployment
- **GIVEN** output context `monitoring` contains Elasticsearch and Kibana services
- **WHEN** a Job requires an output deployment
- **THEN** ESDiag resolves Elasticsearch for Export and Kibana for View from one config load
- **AND** keeps each service's authentication separate

#### Scenario: Output precedence is deterministic
- **GIVEN** `monitoring` is the configured Elastic CLI output context
- **WHEN** a command supplies `.incident.es` in its existing output positional
- **THEN** the explicit output target takes precedence
- **AND** context `incident` resolves as an output deployment for that stage
- **AND** no additional output-context option is required

#### Scenario: Missing configured output fails closed
- **GIVEN** no explicit output or configured output deployment exists
- **WHEN** `elastic diag process .es` requires Export
- **THEN** the command fails instead of using the active input context as output

### Requirement: Elastic Cloud Admin Context Target References
An Elastic CLI Cloud service SHALL be treated as management-plane credentials, not as an `Application` or a directly collectable target. ESDiag SHALL support Cloud-admin proxy collection through `.cloud/<deployment-id>/<application>` for the active context and `.context.cloud/<deployment-id>/<application>` for a named context.

The application segment SHALL be required and SHALL accept a supported application name or alias. Initially, the Cloud admin proxy SHALL support only `es` and `elasticsearch`. Unsupported proxy applications SHALL fail clearly. The existing saved-host template reference `<saved-template>://<deployment-id>[/<application>]` SHALL remain supported as an independent target form.

#### Scenario: Named Cloud context selects Elasticsearch explicitly
- **GIVEN** context `prod` contains a properly scoped Cloud API key
- **WHEN** the user resolves `.prod.cloud/415715723947/es`
- **THEN** ESDiag uses the context's Cloud credentials
- **AND** materializes an Elasticsearch target through the Cloud admin proxy route
- **AND** the resolved target carries an `ElasticCloudHosted` platform hint

#### Scenario: Active Cloud context reference
- **GIVEN** the active Elastic CLI context contains Cloud credentials
- **WHEN** the user resolves `.cloud/415715723947/elasticsearch`
- **THEN** ESDiag uses the active context's Cloud service to construct the proxy target

#### Scenario: Incomplete Cloud reference is not a target
- **WHEN** the user resolves `.cloud`, `.prod.cloud`, or `.prod.cloud/415715723947` without all required selectors
- **THEN** the command fails with guidance requiring both a deployment identifier and application
- **AND** ESDiag does not treat the Cloud management API URL as an application endpoint

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

### Requirement: Extension Documentation
The system SHALL document how to install and use ESDiag through the Elastic CLI extension system, including the shared execution layer and the intentionally smaller `elastic diag` command profile.

#### Scenario: User reads extension installation docs
- **WHEN** a user reads the command-line documentation
- **THEN** the documentation includes an Elastic CLI extension installation example
- **AND** the documentation states that `elastic diag` and `esdiag` use the same Rust execution layer but expose different command profiles
- **AND** the documentation lists the supported Elastic CLI context environment variables consumed by ESDiag

### Requirement: Elastic CLI Context-Aware Help
When the extension profile is active, help output SHALL use the `elastic diag` command name and describe only extension-supported commands and target references. Context-aware help SHALL NOT be required for shell completion behavior.

#### Scenario: Help includes Elastic CLI examples under extension invocation
- **GIVEN** the binary was invoked as `elastic-diag` or the compatibility override is set
- **WHEN** the user runs `elastic diag` without a subcommand
- **THEN** help output includes Elastic CLI-specific examples or target reference guidance

#### Scenario: Delegated command help uses the Clap help subcommand
- **WHEN** the user runs `elastic diag help process`
- **THEN** the extension displays help for `elastic diag process`
- **AND** documentation does not require `elastic diag --help`, because current Elastic CLI releases consume `--help` before extension dispatch

#### Scenario: Standalone help remains focused on esdiag
- **GIVEN** the binary is invoked as `esdiag` without the compatibility override
- **WHEN** the user runs `esdiag --help`
- **THEN** help output remains focused on standalone ESDiag usage
