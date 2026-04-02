## MODIFIED Requirements

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
