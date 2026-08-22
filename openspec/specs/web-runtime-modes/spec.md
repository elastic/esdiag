# web-runtime-modes

## Purpose

Defines explicit runtime modes (`service` and `user`) for the web interface, governing authentication, credential persistence, host configuration, and exporter behavior across both `serve` and desktop-hosted variants.

## Requirements

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
When runtime mode is `service`, the system SHALL enforce shared-instance behavior:
disable local credential persistence, skip reads and writes to `hosts.yml`, `jobs.yml`,
and similar local artifacts, expose only limited user preferences, use a single exporter
defined at startup, and omit local-artifact-backed web UI features even when they are
listed in `ESDIAG_WEB_FEATURES`. Request authentication SHALL NOT be implied by the mode;
it is governed by the separate pluggable authentication axis, so `service` mode MAY run
with any configured authentication provider or with none. The tenancy-driven capability
lockdown (no shared keystore, no user-editable exporter, single startup exporter, all
processed diagnostics to the one shared cluster) SHALL remain a total function of the
mode and MUST NOT be re-granted by any authentication configuration.

#### Scenario: Service mode request processing
- **GIVEN** the web server is running in `service` mode
- **WHEN** a user submits a web request that requires identity and export configuration
- **THEN** the system resolves identity from the configured authentication provider and processes the request using the startup-defined exporter
- **AND** the system does not read or write `hosts.yml`, `jobs.yml`, or other local persistent artifacts

#### Scenario: Optional user web features omitted in service mode
- **GIVEN** the web server is running in `service` mode
- **AND** `ESDIAG_WEB_FEATURES=advanced,job-builder` is set
- **WHEN** the user requests `/advanced`, `/jobs`, or `/jobs/saved`
- **THEN** the service-mode router does not expose those user-mode pages or saved-job web endpoints

#### Scenario: Capability lockdown holds regardless of authentication
- **GIVEN** the web server is running in `service` mode
- **WHEN** any authentication provider (including none) is configured
- **THEN** the shared keystore and user-editable exporter remain unavailable and all processed diagnostics go to the single startup-defined exporter

### Requirement: Pluggable Authentication Axis
The system SHALL treat request authentication as a provider-agnostic axis configured
independently of runtime mode. Supported providers SHALL include Google identity-aware
proxy, and the design MUST admit additional providers (another identity-aware proxy or
Elastic Cloud SSO) and a `none` provider, without changing the runtime-mode enum. The
selected provider SHALL determine how requests are authenticated and how user identity is
resolved. Authentication SHALL serve both access control (gating a shared instance) and
identity provenance: the authenticated identity MUST populate `Identifiers` (user and
account) on bundles and MAY authorize outbound `Send` to the support portal, in either
runtime mode.

#### Scenario: Service mode without an authentication provider
- **GIVEN** the web server starts in `service` mode with authentication provider `none`
- **WHEN** a request arrives without any identity-aware-proxy header
- **THEN** the request is accepted for local testing and identity resolves to the anonymous default

#### Scenario: Service mode behind an identity-aware proxy
- **GIVEN** the web server starts in `service` mode with an identity-aware-proxy provider configured
- **WHEN** a request arrives
- **THEN** the system MUST resolve the user identity from that provider and MUST reject requests that fail the provider's authentication

#### Scenario: Authenticated identity populates provenance
- **WHEN** a job executes under an authenticated identity in either runtime mode
- **THEN** the resolved user and account MUST be recorded in the bundle's `Identifiers`

### Requirement: Service Mode Job Concurrency Caps
When runtime mode is `service`, the system SHALL enforce a global concurrent-job cap and a
per-`Owner` concurrent-job cap, evaluated against the tracked active-job count, so that one
user or automated client cannot starve the shared server. A job that would exceed either
cap SHALL be rejected or deferred rather than admitted. The system SHALL NOT impose a
per-job memory cap: bounded document channels and bulk count/byte limits already provide
backpressure, and a large job MUST still complete by streaming slowly rather than being
rejected for its size. The mapping from data-source weight to concurrency SHALL be
deployment-tunable policy rather than a hardcoded constant.

#### Scenario: Per-owner cap prevents monopolization
- **GIVEN** the web server is running in `service` mode with a per-`Owner` concurrent-job cap of N
- **AND** one owner already has N active jobs
- **WHEN** that same owner submits another job
- **THEN** the system MUST NOT admit the job as an additional concurrent execution while the owner is at the cap

#### Scenario: Global cap protects the shared server
- **GIVEN** the web server is running in `service` mode at the global concurrent-job cap
- **WHEN** any user submits a new job
- **THEN** the system MUST NOT admit it as an additional concurrent execution until active jobs fall below the global cap

#### Scenario: Large job is never rejected for size
- **GIVEN** the web server is running in `service` mode below both concurrency caps
- **WHEN** a large job is submitted
- **THEN** the job MUST be admitted and MUST be allowed to complete by streaming under channel and bulk backpressure, with no per-job memory cap applied

### Requirement: Deferred Coordinated Output-Cluster Load Budget
The system SHALL treat a coordinated load budget against the shared output cluster as a
future concern that is NOT implemented in this change. Per-job `429` retry remains
uncoordinated across concurrent jobs. A shared export concurrency/rate budget SHALL be
revisited when concurrent-job overlap or automation against the shared instance rises;
that rise is the trigger to add it.

#### Scenario: Deferred budget is recorded, not implemented
- **WHEN** this change is implemented
- **THEN** no cross-job export load budget SHALL be added
- **AND** the deferred budget and its rising-overlap trigger MUST remain recorded as a future concern

### Requirement: User Mode Behavior Contract
When runtime mode is `user`, the system SHALL enforce single-user local behavior: no external auth requirement by default, allow saved credentials, permit reading and writing `hosts.yml`, `jobs.yml`, and related local artifacts, provide configurable user settings, allow exporter changes at runtime, and expose optional web pages according to `ServerPolicy` web feature decisions.

#### Scenario: User mode settings and exporter updates
- **GIVEN** the web server is running in `user` mode
- **WHEN** the user updates host credentials and exporter preferences through the UI
- **THEN** the system persists allowed local artifacts and applies exporter changes to subsequent operations without restart

#### Scenario: Advanced visible by default in user mode
- **GIVEN** the web server is running in `user` mode
- **AND** `ESDIAG_WEB_FEATURES` is unset
- **WHEN** the user views the header navigation
- **THEN** the Advanced link is rendered
- **AND** the Job Builder link is not rendered

#### Scenario: Job Builder visible when explicitly enabled
- **GIVEN** the web server is running in `user` mode
- **AND** `ESDIAG_WEB_FEATURES=advanced,job-builder` is set
- **WHEN** the user views the header navigation
- **THEN** both the Advanced and Job Builder links are rendered

### Requirement: CLI Behavior Isolation
Runtime mode behavior SHALL apply only to the web interface and MUST NOT change CLI command behavior.

#### Scenario: CLI command remains unchanged
- **GIVEN** a user runs a CLI command outside web execution
- **WHEN** runtime mode features are present in the codebase
- **THEN** CLI execution semantics and outputs remain unchanged by `service` and `user` mode logic

### Requirement: Mode-Aware Remote Collection Inputs
The Advanced page routes SHALL be mounted only when `ServerPolicy` allows the `advanced` web feature. Within that user-mode Advanced workflow, `Collect -> Collect` SHALL allow selecting from saved known hosts.

#### Scenario: User mode remote collection uses saved host
- **GIVEN** the web interface is running in `user` mode
- **AND** the `advanced` web feature is enabled
- **WHEN** the user selects `Collect -> Collect` in the `Collect` panel
- **THEN** the UI offers saved known hosts as selectable remote collection sources

#### Scenario: Advanced workflow route uses advanced URL
- **GIVEN** the web interface is running in `user` mode
- **AND** the `advanced` web feature is enabled
- **WHEN** the user requests `/advanced`
- **THEN** the Advanced workflow page is rendered

#### Scenario: Workflow URL is not retained
- **GIVEN** the web interface is running in `user` mode
- **AND** the `advanced` web feature is enabled
- **WHEN** the user requests `/workflow`
- **THEN** the server does not mount that route
- **AND** the server does not redirect to `/advanced`

#### Scenario: Service mode does not mount advanced workflow routes
- **GIVEN** the web interface is running in `service` mode
- **WHEN** the user requests `/advanced`
- **THEN** the server does not mount that route
- **AND** advanced workflow configuration is deferred until a future design pass

### Requirement: Mode-Aware Bundle Persistence
The user-mode staged workflow SHALL support browser-managed bundle downloads without requiring a user-configured local filesystem save path.

#### Scenario: User mode exposes browser download save behavior
- **GIVEN** the web interface is running in `user` mode
- **WHEN** the user enables `Save Bundle`
- **THEN** the workflow uses browser-managed download behavior
- **AND** the workflow does not require manual local path entry before execution

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
