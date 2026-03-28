## MODIFIED Requirements

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
