## Purpose

Define secure, optional host credential storage and resolution behavior.

## ADDED Requirements

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
