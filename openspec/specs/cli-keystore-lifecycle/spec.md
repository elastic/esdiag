## Purpose

Define CLI keystore lifecycle commands, unlock lease handling, and secret mutation behavior.

## Requirements

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

### Requirement: Unlock Delegates Use Without Disclosure
Unlocking the keystore SHALL create a time-limited grant that lets ESDiag *use* saved-host
credentials to collect and process, WITHOUT ever exposing the plaintext to the caller. A
delegated actor — automation, or an LLM agent — MAY drive ESDiag to collect and process
through it during the unlock window but MUST NOT be able to read any saved credential in
plaintext. The grant SHALL be rate-limited against unlock-password brute force
(`KeystoreRateLimit`), and the same use-without-disclosure guarantee SHALL apply whether
the unlock password was entered via the CLI or the Web UI.

As a **load-bearing** invariant, no ESDiag interface SHALL return a saved credential in
plaintext, and credential material SHALL be held in a wrapper whose debug and display
forms render a redaction marker and whose serialization is opt-in per field, so that a
new log line or event field cannot disclose a secret by accident. Any change to unlock,
key derivation, or the credential-carrying types MUST preserve this property.

The unlock envelope is encrypted under a key derived from machine context rather than
from the keystore password, because unattended use gives ESDiag no secret to decrypt
with. The guarantee that follows is scoped accordingly: the file holds no plaintext at
rest and does not decrypt under a different machine context, so exfiltrating the bytes
alone yields nothing. Code running on the host as the owning user is explicitly **out
of scope** — it can reconstruct the context — and local file permissions bound that
case instead (ADR-0012).

#### Scenario: Delegated actor uses credentials without reading them
- **GIVEN** the keystore is unlocked with a valid unexpired lease
- **WHEN** a delegated actor drives ESDiag to collect from a saved host during the unlock window
- **THEN** ESDiag performs the credentialed collection on the actor's behalf
- **AND** the actor never receives the saved credential in plaintext

#### Scenario: Exfiltrated unlock file yields no usable credential
- **GIVEN** an attacker obtains the `keystore.unlock` file, and optionally the encrypted keystore file
- **AND** the attacker does not possess the keystore password or the originating machine context
- **WHEN** the attacker attempts to reconstruct a saved credential from those files
- **THEN** the unlock envelope does not decrypt and no plaintext credential can be derived

#### Scenario: Credential material is redacted in debug and log output
- **GIVEN** any type that carries an API key, a password, or the cached keystore password
- **WHEN** a value of that type is formatted for a log line, an error, or an event
- **THEN** the rendered output contains a redaction marker rather than the credential

#### Scenario: Expired lease revokes delegated use
- **GIVEN** the unlock lease has expired
- **WHEN** a delegated actor attempts to drive a credentialed operation
- **THEN** ESDiag treats the keystore as locked and does not use any saved credential until a new valid password source is supplied

#### Scenario: Unlock brute force is rate limited
- **WHEN** repeated unlock attempts are made with incorrect passwords
- **THEN** the system rate-limits further unlock attempts to contain brute-force guessing of the unlock password
