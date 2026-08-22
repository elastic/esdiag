## Purpose

Define secure, optional host credential storage and resolution behavior.

## Requirements

### Requirement: Encrypted Host Secret Storage
The system SHALL support storing host authentication secrets in an encrypted local keystore file that is separate from `hosts.yml`. The keystore SHALL store passwords, API keys, and related authentication material using a secret identifier (`secret_id`) key. When a host references a `secret_id`, the system SHALL resolve the keystore password by checking, in order, the scoped in-memory password, `ESDIAG_KEYSTORE_PASSWORD`, and a valid unexpired CLI unlock lease before decrypting the keystore.

#### Scenario: Resolve credentials using secret identifier
- **GIVEN** a host entry includes `secret: "prod-es-main"`
- **AND** the keystore contains an encrypted secret record with ID `prod-es-main`
- **AND** a valid keystore password is available from scoped state, the environment variable, or a valid CLI unlock lease
- **WHEN** the system loads host configuration for an operation
- **THEN** the system decrypts and resolves credentials from the keystore record
- **AND** the system uses those credentials for host authentication

#### Scenario: Resolve credentials using unlock lease fallback
- **GIVEN** a host entry includes `secret: "prod-es-main"`
- **AND** the keystore contains an encrypted secret record with ID `prod-es-main`
- **AND** no scoped password or `ESDIAG_KEYSTORE_PASSWORD` is present
- **AND** `~/.esdiag/keystore.unlock` contains a valid unexpired unlock lease
- **WHEN** the system loads host configuration for an operation
- **THEN** the system decrypts the unlock lease
- **AND** the system uses the cached keystore password to decrypt the keystore record

### Requirement: Optional Secret Store Adoption
The system SHALL keep secret-store usage optional while changing the steady-state saved host format to persist either a secret reference or no auth state. The system SHALL continue reading legacy tagged host records and legacy plaintext auth fields for compatibility, but newly written host records SHALL NOT require the `auth` tag and SHALL NOT continue writing legacy inline auth fields in the new format.

#### Scenario: Read a legacy plaintext host without prior migration
- **GIVEN** a host entry does not include a `secret` value
- **AND** the host entry includes legacy plaintext authentication fields in the old tagged format
- **WHEN** the system loads host configuration
- **THEN** the system accepts the legacy record for compatibility
- **AND** the legacy auth fields remain available to compatibility-sensitive flows such as validation and migration

#### Scenario: Rewrite a host into the new saved format
- **GIVEN** a host is saved or rewritten by the current application version
- **WHEN** the system writes the host record to `hosts.yml`
- **THEN** the record omits the legacy `auth` tag
- **AND** the record persists a `secret` reference only when one is configured
- **AND** a record without a persisted `secret` reference is treated as a no-auth saved host record only when the host does not require authentication

### Requirement: Secret Identifier Integrity
The system SHALL fail configuration validation when a host explicitly references a `secret_id` that is missing or unreadable in the keystore. The system SHALL also reject attempts to remove a secret that is still referenced by any saved host or by any saved job that depends on a host using that secret.

#### Scenario: Referenced secret is missing
- **GIVEN** a host entry includes `secret: "missing-secret"`
- **AND** the keystore does not contain `missing-secret`
- **WHEN** the system validates configuration
- **THEN** validation fails with an explicit error that identifies the missing `secret_id`

#### Scenario: Secret deletion blocked by host reference
- **GIVEN** a saved host references `secret: "prod-es-main"`
- **WHEN** the user attempts to remove `prod-es-main` from the keystore
- **THEN** the operation fails with an explicit error identifying the referencing host

#### Scenario: Secret deletion blocked by saved job reference
- **GIVEN** a saved job references a known host that uses `secret: "prod-es-main"`
- **WHEN** the user attempts to remove `prod-es-main` from the keystore
- **THEN** the operation fails with an explicit error identifying the referencing saved job

#### Scenario: Explicit secret and legacy credentials both exist
- **GIVEN** a host entry includes `secret: "prod-es-main"`
- **AND** the same host entry also includes legacy plaintext credentials
- **AND** the keystore contains `prod-es-main`
- **WHEN** the system resolves host authentication
- **THEN** the system authenticates using the keystore secret
- **AND** logs a warning that legacy plaintext credentials are being ignored

### Requirement: Legacy Host Migration Support
The system SHALL preserve full `keystore migrate` support for legacy hosts that still contain tagged auth state or inline plaintext credentials. Migration SHALL read legacy auth fields, write equivalent secret entries to the keystore, update each migrated host to reference its secret identifier, and rewrite the host record in the new flat format.

#### Scenario: Migrate a legacy API key host
- **GIVEN** a legacy saved host contains an inline API key in the old tagged host format
- **WHEN** the user runs `esdiag keystore migrate`
- **THEN** the system writes the API key into the keystore under the migrated secret identifier
- **AND** rewrites the host to reference that secret identifier in the new saved host format
- **AND** removes the legacy inline API key fields from the rewritten host record

#### Scenario: Migrate a legacy basic auth host
- **GIVEN** a legacy saved host contains inline username and password fields in the old tagged host format
- **WHEN** the user runs `esdiag keystore migrate`
- **THEN** the system writes the username and password into the keystore under the migrated secret identifier
- **AND** rewrites the host to reference that secret identifier in the new saved host format
- **AND** removes the legacy inline username and password fields from the rewritten host record

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
the execution that consumed it. The key SHALL be held in a redacting wrapper so that
formatting the value that carries it, for a log line or an event, cannot disclose it.

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
