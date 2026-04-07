## MODIFIED Requirements

### Requirement: Optional Secret Store Adoption
The system SHALL keep secret-store usage optional while changing the steady-state saved host format to persist either a secret reference or no auth state. The system SHALL continue reading legacy tagged host records and legacy plaintext auth fields for compatibility, but newly written host records SHALL NOT require the `auth` tag and SHALL NOT continue writing legacy inline auth fields in the new format.

#### Scenario: Read a legacy plaintext host without prior migration
- **GIVEN** a legacy host entry does not include a `secret` value
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

## ADDED Requirements

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
