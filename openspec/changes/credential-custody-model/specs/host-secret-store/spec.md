## ADDED Requirements

### Requirement: Credential Direction Classification
The system SHALL classify every credential by stage **direction**: an *input* credential
authenticates to a source being collected (a `Collect` stage), and an *output* credential
authenticates to a destination being written to (a `Send` or `Export` stage, or a `View`
Kibana link). The encrypted keystore SHALL be role-agnostic — a saved known host persists
its credential regardless of direction — and direction SHALL be derived from the
referencing host or stage, never stored as a store-level attribute.

#### Scenario: Input credential for a saved collect host
- **GIVEN** a saved known host used as a `Collect` source references `secret: "prod-es-main"`
- **WHEN** the system resolves credentials for the collection
- **THEN** the resolved credential is treated as an *input* credential for that stage
- **AND** the keystore record itself carries no direction attribute

#### Scenario: Output credential for a saved destination host
- **GIVEN** a saved known host used as a `Send`/`Export`/`View` destination references `secret: "support-portal"`
- **WHEN** the system resolves credentials for the destination
- **THEN** the resolved credential is treated as an *output* credential for that stage
- **AND** the same keystore is used to store it as is used for input credentials

### Requirement: User-Mode-Only Credential Persistence
The system SHALL persist credentials at the application layer **only in `User` mode**.
In `User` mode, credentials for saved known hosts of any direction SHALL persist in the
encrypted keystore, while ad-hoc user-provided keys SHALL be runtime-only. In `Service`
mode the application SHALL persist no credentials server-side: output credentials SHALL be
injected from a vault or secrets service into environment variables at container runtime,
user identity SHALL be established by the identity-aware proxy rather than the application,
and input keys SHALL be ephemeral. A compromised `Service`-mode container image or config
file MUST therefore yield no stored credentials. This server-side invariant does not
constrain credential persistence on the user's own device, which is a separate axis.

#### Scenario: Saved host credential persists in User mode
- **GIVEN** the application is running in `User` mode
- **WHEN** the user saves a known host with authentication material
- **THEN** the credential is written to the encrypted keystore under a `secret_id`
- **AND** ad-hoc keys entered for a one-off operation are not written to the keystore

#### Scenario: Service mode persists no credential server-side
- **GIVEN** the application is running in `Service` mode
- **WHEN** a job collects from an input source and exports to the shared output cluster
- **THEN** the output credential is read from a runtime-injected environment variable and never written to any application-layer store
- **AND** the input key is held only for the duration of the execution and never persisted server-side

#### Scenario: Compromised service container exposes no stored secret
- **GIVEN** the application is running in `Service` mode
- **WHEN** an attacker reads the container image and its on-disk configuration
- **THEN** no persisted credential of any direction is recoverable, because the application stored none

### Requirement: Ad-hoc Input Key Non-Leakage
An ad-hoc input API key provided at runtime on the shared service SHALL be one-time-use for
a single execution and MUST NEVER be persisted, written to logs, or included in any event —
including the broadcast and targeted events defined by ADR-0008. The key MUST NOT survive
the execution that consumed it.

#### Scenario: Ad-hoc input key is never persisted or logged
- **GIVEN** a user supplies an ad-hoc input API key for a single `Collect` execution on the shared service
- **WHEN** the execution runs to completion or fails
- **THEN** the key is not written to any keystore, host record, or other on-disk artifact
- **AND** the key does not appear in any log line at any level

#### Scenario: Ad-hoc input key is excluded from events
- **GIVEN** a job uses an ad-hoc input API key on the shared service
- **WHEN** the system emits broadcast or targeted job events
- **THEN** no event payload contains the input key or any reconstructable form of it

#### Scenario: Ad-hoc input key does not outlive its execution
- **GIVEN** an ad-hoc input API key was used for one execution
- **WHEN** a subsequent execution or request is made
- **THEN** the earlier key is unavailable and the user must supply a key again

### Requirement: Custody Backend Independent of Runtime Mode
The system SHALL treat the credential custody *backend* (where a secret lives) as an axis
independent of the runtime *mode* (who runs ESDiag). Current backends are the encrypted
file keystore (`User` mode), vault-to-environment injection (`Service` output), and
ephemeral runtime storage (input). An OS-native keystore is a deferred candidate backend
that is not implemented, and adopting one MUST NOT re-bind the backend axis to mode.

#### Scenario: Backend does not follow from mode alone
- **GIVEN** the runtime mode is known
- **WHEN** the system selects a custody backend for a credential
- **THEN** the selection is determined by the credential's direction and configuration, not by the runtime mode as a proxy for the backend

#### Scenario: OS-native backend is not offered
- **WHEN** a user inspects the available custody backends in the current release
- **THEN** no OS-native keystore backend is presented, because it remains deferred
