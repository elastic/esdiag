## ADDED Requirements

### Requirement: CLI Unlock Lease Commands
The system SHALL provide `esdiag keystore unlock`, `esdiag keystore lock`, and `esdiag keystore status` commands for CLI-managed keystore access. `unlock` SHALL create or refresh local unlock state in an unlock file stored alongside the active keystore path, `lock` SHALL clear local unlock state, and `status` SHALL report whether the local keystore is present and whether CLI unlock state is active.

#### Scenario: Unlock creates active lease
- **WHEN** the user runs `esdiag keystore unlock` with a valid keystore password
- **THEN** the system creates or refreshes an unlock file named `keystore.unlock` alongside the active keystore path
- **AND** subsequent CLI runs may use that lease until it expires or is explicitly locked

#### Scenario: Lock clears active lease
- **WHEN** the user runs `esdiag keystore lock`
- **THEN** the system removes the unlock file alongside the active keystore path when it exists
- **AND** later CLI runs require another valid password source before decrypting keystore-backed secrets

#### Scenario: Status reports unlocked lease
- **GIVEN** the keystore exists and the unlock file alongside the active keystore path contains a valid unexpired lease
- **WHEN** the user runs `esdiag keystore status`
- **THEN** the system reports that the keystore is present
- **AND** the system reports that CLI unlock state is active with the lease expiration time

### Requirement: Unlock Lease TTL Validation
The system SHALL write unlock leases with an `expires_at_epoch` value, SHALL default `esdiag keystore unlock` to a 24-hour lease, SHALL accept `--ttl` values as integer plus a single-character suffix (`m`, `h`, or `d`), and SHALL reject durations longer than 30 days.

#### Scenario: Default unlock TTL
- **WHEN** the user runs `esdiag keystore unlock` without `--ttl`
- **THEN** the created lease expires 24 hours after unlock time

#### Scenario: Custom TTL within limit
- **WHEN** the user runs `esdiag keystore unlock --ttl 7d`
- **THEN** the created lease expires 7 days after unlock time

#### Scenario: TTL above maximum is rejected
- **WHEN** the user runs `esdiag keystore unlock --ttl 31d`
- **THEN** the command fails with a validation error
- **AND** no unlock lease is written

#### Scenario: Expired unlock lease is deleted on read
- **GIVEN** the unlock file alongside the active keystore path contains an expiration timestamp in the past
- **WHEN** an `esdiag` command checks the unlock lease
- **THEN** the system treats the keystore as locked
- **AND** the system deletes the expired unlock file on a best-effort basis

### Requirement: Unlock Lease Confidentiality and Resilience
The system SHALL store the cached keystore password in an unlock file named `keystore.unlock` alongside the active keystore path using a minimally encrypted envelope rather than plaintext, SHALL create the file with restrictive local permissions when supported by the platform, and SHALL treat malformed or unreadable unlock files as locked state.

#### Scenario: Unlock file does not expose plaintext password
- **WHEN** the system writes the unlock file alongside the active keystore path
- **THEN** the file does not store the cached keystore password in plaintext form

#### Scenario: Corrupt unlock file is ignored
- **GIVEN** the unlock file alongside the active keystore path is malformed or fails to decrypt
- **WHEN** an `esdiag` command checks the unlock lease
- **THEN** the system treats the keystore as locked
- **AND** the command may warn about the invalid unlock file

### Requirement: Interactive Unlock Bootstrap
When no encrypted keystore exists, `esdiag keystore unlock` SHALL prompt the user to confirm keystore creation in an interactive terminal before creating a new keystore. In non-interactive execution, the command SHALL warn and exit without creating a keystore.

#### Scenario: Interactive unlock offers bootstrap
- **GIVEN** no keystore file exists
- **AND** the command is running in an interactive terminal
- **WHEN** the user runs `esdiag keystore unlock`
- **THEN** the system prompts to confirm keystore creation before writing a new keystore

#### Scenario: Non-interactive unlock refuses bootstrap
- **GIVEN** no keystore file exists
- **AND** the command is not running in an interactive terminal
- **WHEN** the user runs `esdiag keystore unlock`
- **THEN** the command exits with a warning
- **AND** no keystore or unlock file is created

### Requirement: Keystore Password Rotation
The system SHALL provide `esdiag keystore password` to rotate the keystore password by validating the current password, prompting for a new password, and re-encrypting the existing keystore contents with the new password.

#### Scenario: Password rotation succeeds
- **GIVEN** an encrypted keystore already exists
- **WHEN** the user runs `esdiag keystore password` and provides the correct current password plus a valid new password
- **THEN** the system rewrites the keystore using the new password
- **AND** existing secret records remain available after rotation

#### Scenario: Password rotation fails when keystore is absent
- **GIVEN** no encrypted keystore exists
- **WHEN** the user runs `esdiag keystore password`
- **THEN** the command fails with a message that no keystore exists

### Requirement: Explicit Secret Creation and Update Semantics
The system SHALL make `esdiag keystore add <secret_id>` create-only and SHALL provide `esdiag keystore update <secret_id>` for changes to an existing secret. `add` MUST fail when the secret already exists, and `update` MUST fail when the secret does not exist.

#### Scenario: Add rejects duplicate secret
- **GIVEN** the keystore already contains secret `prod-es`
- **WHEN** the user runs `esdiag keystore add prod-es --apikey abc123`
- **THEN** the command fails with an error that the secret already exists
- **AND** the existing secret remains unchanged

#### Scenario: Update rejects missing secret
- **GIVEN** the keystore does not contain secret `prod-es`
- **WHEN** the user runs `esdiag keystore update prod-es --apikey abc123`
- **THEN** the command fails with an error that the secret was not found

#### Scenario: Update replaces existing secret payload
- **GIVEN** the keystore already contains secret `prod-es`
- **WHEN** the user runs `esdiag keystore update prod-es --user elastic --password new-pass`
- **THEN** the command updates the stored secret for `prod-es`
- **AND** later host resolution uses the updated secret value

### Requirement: Interactive Secret Material Prompting
For `esdiag keystore add` and `esdiag keystore update`, the system SHALL allow explicit API key and password values on the command line, but when required secret material is absent in an interactive terminal the CLI SHALL prompt for it using masked input. In non-interactive execution, the command MUST fail when required secret material is missing.

#### Scenario: Add prompts for missing API key
- **GIVEN** the command is running in an interactive terminal
- **WHEN** the user runs `esdiag keystore add prod-es --apikey`
- **THEN** the CLI prompts for the API key using masked input
- **AND** the provided value is used for the new secret

#### Scenario: Update prompts for missing password
- **GIVEN** the command is running in an interactive terminal
- **WHEN** the user runs `esdiag keystore update prod-es --user elastic --password`
- **THEN** the CLI prompts for the password using masked input
- **AND** the provided value is used for the updated secret

#### Scenario: Non-interactive add fails when secret material is missing
- **GIVEN** the command is not running in an interactive terminal
- **WHEN** the user runs `esdiag keystore add prod-es --apikey`
- **THEN** the command fails with an error that the required secret value was not provided
