## ADDED Requirements

### Requirement: Elasticrc Workspace Crate
The system SHALL provide native Elastic CLI config support through a dedicated Rust library crate named `elasticrc` in the ESDiag Cargo workspace. The crate SHALL own Elastic CLI config discovery, parsing, validation, resolver expressions, OS secret resolution, and inline-secret permission warnings. The main `esdiag` crate SHALL consume `elasticrc` outputs to construct transient ESDiag targets.

#### Scenario: Workspace exposes elasticrc library
- **WHEN** the project is built
- **THEN** Cargo recognizes an `elasticrc` workspace library crate
- **AND** the existing `esdiag` binary package remains available with the same package name and binary name

#### Scenario: ESDiag converts resolved service block
- **GIVEN** `elasticrc` resolves a context service block containing URL and authentication data
- **WHEN** ESDiag consumes that resolved service block
- **THEN** ESDiag constructs a transient target for collection or output
- **AND** ESDiag does not persist that target to host storage only because it came from `elasticrc`

### Requirement: Elasticrc Feature Gate
The main ESDiag crate SHALL expose native Elastic CLI config support behind an `elasticrc` Cargo feature. The `elasticrc` feature SHALL be enabled by the default feature set.

#### Scenario: Default build includes elasticrc
- **WHEN** the project is built with default features
- **THEN** native Elastic CLI config target resolution is available

#### Scenario: Build without elasticrc omits native config resolution
- **WHEN** the project is built without the `elasticrc` feature
- **THEN** `.context.service` native config resolution is unavailable
- **AND** active `.service` environment-backed references may still work when their required environment variables are present

### Requirement: Root Cargo Install Compatibility
The workspace layout SHALL preserve repo-root Cargo install and build behavior for the existing ESDiag binary.

#### Scenario: Cargo install from repository root
- **WHEN** a user runs `cargo install --path .` at the repository root
- **THEN** Cargo installs the existing `esdiag` binary

### Requirement: Keyring-Core Credential Boundary
The `elasticrc` crate SHALL use `keyring-core` as the credential access abstraction for OS-backed secret resolution. Platform-specific credential store integrations SHOULD use keyring-compatible native store crates where practical. Implementation MAY use examples from the `keyring` crate to select and configure stores.

#### Scenario: Credential resolver uses keyring-core abstraction
- **GIVEN** an Elastic CLI config references an OS-backed secret resolver
- **WHEN** `elasticrc` resolves the secret
- **THEN** credential lookup flows through the `keyring-core` abstraction
- **AND** platform-specific lookup details remain encapsulated in the `elasticrc` crate

#### Scenario: Native store crate is unavailable
- **GIVEN** no native keyring-compatible store is available for the current platform or environment
- **WHEN** `elasticrc` resolves an OS-backed secret resolver
- **THEN** the resolver fails with a clear platform or store availability error
- **AND** the error does not expose secret values

### Requirement: Secret Redaction
The `elasticrc` crate SHALL wrap resolved secret values with `redact`-based types or equivalent redaction behavior before exposing them through public typed structures.

#### Scenario: Debug output redacts resolved secret
- **GIVEN** `elasticrc` resolves an API key secret
- **WHEN** the resolved auth structure is formatted for debug output
- **THEN** the secret value is not shown in plaintext

### Requirement: Elastic CLI Config Discovery
The system SHALL support reading Elastic CLI configuration files for named-context target resolution. The resolver SHALL discover the same default file names as the Elastic CLI in the user's home directory and SHALL support an explicit config-file override.

#### Scenario: Discover default Elastic CLI config
- **GIVEN** the user has one of `.elasticrc`, `.elasticrc.json`, `.elasticrc.yaml`, or `.elasticrc.yml` in their home directory
- **WHEN** ESDiag resolves a named Elastic context target reference
- **THEN** the system reads the first readable Elastic CLI config using Elastic CLI discovery order

#### Scenario: Use explicit Elastic CLI config path
- **GIVEN** an explicit Elastic CLI config file path is configured for the ESDiag invocation
- **WHEN** ESDiag resolves a named Elastic context target reference
- **THEN** the system reads the Elastic CLI config from the explicit path instead of home-directory discovery

#### Scenario: Use Elastic CLI config environment override
- **GIVEN** `ELASTIC_CLI_CONFIG_FILE` is set to a readable config file path
- **WHEN** ESDiag resolves a named Elastic context target reference
- **THEN** the system reads the Elastic CLI config from `ELASTIC_CLI_CONFIG_FILE` instead of home-directory discovery

#### Scenario: Reject executable config formats
- **GIVEN** the configured Elastic CLI config path ends with `.js`, `.ts`, `.mjs`, or `.cjs`
- **WHEN** ESDiag attempts to load the config
- **THEN** the system rejects the config file
- **AND** the error explains that executable config formats are not supported

### Requirement: Elastic CLI Config Shape Validation
The system SHALL validate Elastic CLI config structure before resolving named-context target references. A valid config MUST include a `current_context` string and a non-empty `contexts` map. Each resolved service block MUST include an HTTP or HTTPS URL and MAY include API key or basic authentication.

#### Scenario: Missing context is rejected
- **GIVEN** Elastic CLI configuration does not contain context `prod`
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the command fails with an error that names the missing context
- **AND** the error lists available contexts when they are known

#### Scenario: Missing service is rejected
- **GIVEN** Elastic CLI configuration contains context `prod`
- **AND** context `prod` does not contain a Kibana service block
- **WHEN** ESDiag resolves `.prod.kb`
- **THEN** the command fails with an error that names the missing service and context

#### Scenario: Invalid service URL is rejected
- **GIVEN** Elastic CLI configuration contains context `prod`
- **AND** `prod.elasticsearch.url` is not an HTTP or HTTPS URL
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the command fails with a validation error for the service URL

### Requirement: Named Elastic Context Target References
The system SHALL support leading-dot named-context target references for command arguments that can resolve remote targets. A reference of `.context.service` SHALL resolve the named service from the named Elastic CLI context. The rightmost segment MUST identify a known Elastic CLI service name or alias supported by ESDiag.

The service aliases MUST resolve as follows:
- `es` resolves to `elasticsearch`
- `kb` resolves to `kibana`
- `cloud` resolves to `cloud`

#### Scenario: Process resolves explicit source and output contexts
- **GIVEN** Elastic CLI configuration contains contexts named `prod` and `diag`
- **AND** each context contains an Elasticsearch service
- **WHEN** the user runs `elastic diag process .prod.elasticsearch .diag.es`
- **THEN** the process input resolves to the `elasticsearch` service from context `prod`
- **AND** the process output resolves to the `elasticsearch` service from context `diag`

#### Scenario: Dotted context name resolves from rightmost service segment
- **GIVEN** Elastic CLI configuration contains a context named `prod.us-west`
- **AND** that context contains an Elasticsearch service
- **WHEN** the user runs `esdiag process .prod.us-west.es .diag.es`
- **THEN** `.prod.us-west.es` resolves to the `elasticsearch` service from context `prod.us-west`

#### Scenario: Resolver supports Kibana alias
- **GIVEN** Elastic CLI configuration contains a Kibana service
- **WHEN** a command resolves `.prod.kb`
- **THEN** the target service is interpreted as `kibana`

#### Scenario: Resolver supports Cloud service
- **GIVEN** Elastic CLI configuration contains a Cloud service
- **WHEN** a command resolves `.prod.cloud`
- **THEN** the target service is interpreted as `cloud`
- **AND** ESDiag uses the existing Cloud API key target path for that transient target

#### Scenario: Unsupported service alias falls through
- **GIVEN** Elastic CLI configuration does not define Logstash as a supported service type
- **WHEN** a command resolves `.prod.ls`
- **THEN** the system does not treat `ls` as an Elastic context target reference
- **AND** the argument continues through saved-host, URL, local file, directory, and stream resolution

#### Scenario: Elastic context target takes precedence over saved host
- **GIVEN** Elastic CLI configuration contains context `prod` with an Elasticsearch service
- **AND** ESDiag saved hosts contain a host named `.prod.es`
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the Elastic CLI config target is used
- **AND** the saved host of the same name is ignored for that argument

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

### Requirement: Elastic CLI Config Authentication
The system SHALL translate supported Elastic CLI service authentication blocks into transient ESDiag remote targets without writing those credentials to `~/.esdiag/hosts.yml` or the ESDiag keystore.

#### Scenario: API key authentication resolves from named context
- **GIVEN** Elastic CLI configuration contains context `prod`
- **AND** `prod.elasticsearch.auth.api_key` is configured
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the transient target uses API key authentication from the Elastic CLI config
- **AND** the credential is not persisted to ESDiag host storage

#### Scenario: Basic authentication resolves from named context
- **GIVEN** Elastic CLI configuration contains context `prod`
- **AND** `prod.elasticsearch.auth.username` and `prod.elasticsearch.auth.password` are configured
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the transient target uses basic authentication from the Elastic CLI config
- **AND** the credential is not persisted to ESDiag host storage

#### Scenario: Unauthenticated service block resolves
- **GIVEN** Elastic CLI configuration contains context `prod`
- **AND** `prod.elasticsearch.url` is configured without an `auth` block
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the transient target uses the configured URL without authentication

### Requirement: Elastic CLI Resolver Expressions
The system SHALL resolve Elastic CLI config expressions before validating a named context target. Resolver expressions use the form `$(resolver:params)` and MAY appear in URL or authentication string fields.

#### Scenario: Environment resolver is supported
- **GIVEN** Elastic CLI configuration contains `api_key: $(env:PROD_ES_API_KEY)`
- **AND** `PROD_ES_API_KEY` is set
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the transient target uses the resolved environment variable value as the API key

#### Scenario: File resolver is supported
- **GIVEN** Elastic CLI configuration contains `api_key: $(file:/run/secrets/prod-api-key)`
- **AND** the file exists, is a regular file, and is within the supported resolver size limit
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the transient target uses the trimmed file contents as the API key

#### Scenario: Command resolver is supported with bounded execution
- **GIVEN** Elastic CLI configuration contains `api_key: $(cmd:pass show elastic/prod-api-key)`
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the system executes the command with a bounded timeout
- **AND** the system executes the command without shell interpretation
- **AND** the transient target uses the trimmed command output as the API key

#### Scenario: Command resolver rejects shell-only syntax
- **GIVEN** Elastic CLI configuration contains a command resolver value that requires shell interpretation
- **WHEN** ESDiag resolves the target reference
- **THEN** the command fails with an error explaining that shell interpretation is unsupported
- **AND** the error does not expose secret values

#### Scenario: Unknown resolver fails clearly
- **GIVEN** Elastic CLI configuration contains `api_key: $(unknown:value)`
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the command fails with an error naming the unknown resolver
- **AND** the error identifies the config field that contained the unresolved expression

### Requirement: OS Secret Resolver Parity
The system SHALL support the Elastic CLI OS secret resolver expressions used for keychain-backed credentials: `$(keychain:service/account)` on macOS, `$(secret_service:service/account)` on Linux, `$(pass:path)` where `pass` is available, and `$(credential_manager:service/account)` on Windows.

#### Scenario: macOS Keychain resolver is supported
- **GIVEN** the platform is macOS
- **AND** Elastic CLI configuration contains `api_key: $(keychain:elastic-cli/prod-api-key)`
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the system reads the secret from macOS Keychain
- **AND** the transient target uses that value as the API key

#### Scenario: Linux Secret Service resolver is supported
- **GIVEN** the platform is Linux
- **AND** Elastic CLI configuration contains `api_key: $(secret_service:elastic-cli/prod-api-key)`
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the system reads the secret from freedesktop Secret Service
- **AND** the transient target uses that value as the API key

#### Scenario: pass resolver is supported
- **GIVEN** Elastic CLI configuration contains `api_key: $(pass:elastic/prod-api-key)`
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the system reads the first line from `pass show elastic/prod-api-key`
- **AND** the transient target uses that value as the API key

#### Scenario: Windows Credential Manager resolver is supported
- **GIVEN** the platform is Windows
- **AND** Elastic CLI configuration contains `api_key: $(credential_manager:elastic-cli/prod-api-key)`
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the system reads the secret from Windows Credential Manager
- **AND** the transient target uses that value as the API key

#### Scenario: Platform-specific resolver rejects unsupported platform
- **GIVEN** Elastic CLI configuration uses a platform-specific resolver on an unsupported operating system
- **WHEN** ESDiag resolves the target reference
- **THEN** the command fails with an error naming the resolver and the unsupported platform

### Requirement: Inline Secret Compatibility
The system SHALL support Elastic CLI config files that store secrets inline, including files created with Elastic CLI `--inline-secrets`, while warning when inline secrets are stored in a config file with loose permissions on platforms where permissions can be evaluated.

#### Scenario: Inline API key resolves
- **GIVEN** Elastic CLI configuration contains an inline `api_key` value
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the transient target uses the inline API key value

#### Scenario: Loose inline-secret config warns
- **GIVEN** the Elastic CLI config file contains inline secrets
- **AND** the platform supports Unix-style permission checks
- **AND** the config file permissions are broader than `0600` or `0400`
- **WHEN** ESDiag loads the config
- **THEN** the system emits a warning that the config file contains inline secrets with loose permissions
- **AND** the target may still resolve if the config is otherwise valid

#### Scenario: Resolver-backed secret does not trigger inline warning
- **GIVEN** the Elastic CLI config file stores secrets only as resolver expressions
- **WHEN** ESDiag loads the config
- **THEN** the system does not warn merely because secret fields are present

### Requirement: Elastic CLI Context Selection Parity
The system SHALL respect Elastic CLI context selection semantics for config loading. The default active context SHALL come from `current_context`, and an explicit context in `.context.service` SHALL select that named context without changing the active context.

#### Scenario: Active context is available for native config workflows
- **GIVEN** Elastic CLI configuration sets `current_context: local`
- **AND** context `local` contains an Elasticsearch service
- **WHEN** ESDiag resolves an active-context reference through native config loading
- **THEN** the resolver uses context `local`

#### Scenario: Explicit named context does not mutate current context
- **GIVEN** Elastic CLI configuration sets `current_context: local`
- **AND** contexts `local` and `prod` both exist
- **WHEN** ESDiag resolves `.prod.es`
- **THEN** the resolver uses context `prod`
- **AND** the config file's `current_context` remains unchanged

### Requirement: Experimental Schema Drift Coverage
The `elasticrc` crate SHALL include fixture-based tests for the Elastic CLI config shapes it supports so schema drift in the experimental upstream Elastic CLI can be detected during ESDiag development.

#### Scenario: Supported fixture resolves
- **GIVEN** a fixture matching the supported Elastic CLI config shape
- **WHEN** `elasticrc` loads the fixture
- **THEN** the expected contexts and services resolve successfully

#### Scenario: Unsupported service is rejected clearly
- **GIVEN** an Elastic CLI config contains a service block unsupported by ESDiag target resolution
- **WHEN** ESDiag resolves a target reference for that service
- **THEN** the command fails or falls through according to the target reference rules
- **AND** tests document the expected behavior
