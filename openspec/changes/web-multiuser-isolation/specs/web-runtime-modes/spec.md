## MODIFIED Requirements

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

## ADDED Requirements

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
