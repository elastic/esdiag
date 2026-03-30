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
The system SHALL keep secret-store usage optional and SHALL continue supporting existing plaintext authentication fields for environments that do not use encrypted secret storage.

#### Scenario: No secret identifier provided
- **GIVEN** a host entry does not include a `secret` value
- **AND** the host entry includes legacy plaintext authentication fields
- **WHEN** the system loads host configuration
- **THEN** the system authenticates using legacy plaintext fields
- **AND** the configuration is treated as valid when role constraints are satisfied

### Requirement: Secret Identifier Integrity
The system SHALL fail configuration validation when a host explicitly references a `secret_id` that is missing or unreadable in the keystore.

#### Scenario: Referenced secret is missing
- **GIVEN** a host entry includes `secret: "missing-secret"`
- **AND** the keystore does not contain `missing-secret`
- **WHEN** the system validates configuration
- **THEN** validation fails with an explicit error that identifies the missing `secret_id`

#### Scenario: Explicit secret and legacy credentials both exist
- **GIVEN** a host entry includes `secret: "prod-es-main"`
- **AND** the same host entry also includes legacy plaintext credentials
- **AND** the keystore contains `prod-es-main`
- **WHEN** the system resolves host authentication
- **THEN** the system authenticates using the keystore secret
- **AND** logs a warning that legacy plaintext credentials are being ignored
