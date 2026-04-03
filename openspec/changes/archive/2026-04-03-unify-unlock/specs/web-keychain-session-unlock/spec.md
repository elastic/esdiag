## MODIFIED Requirements

### Requirement: Secrets Password Unlock for Web Session
The system SHALL require the user to provide the secrets password before any encrypted keychain read or write operation is performed from the web interface.

#### Scenario: Keychain operation attempted while locked
- **WHEN** the user initiates a keychain-backed action and the keystore unlock file is absent or expired
- **THEN** the system prompts for the secrets password and does not perform the keychain operation until unlock succeeds

### Requirement: File-Based Unlock Shared With CLI
The web runtime SHALL write the `keystore.unlock` file on a successful unlock, using the same format and default TTL as `esdiag keystore unlock`. Web lock actions SHALL delete the `keystore.unlock` file. Lock state is determined solely by whether a valid, unexpired `keystore.unlock` file exists on disk; no separate in-memory session state is maintained.

#### Scenario: Successful web unlock writes unlock file
- **WHEN** the user submits a valid secrets password via the web interface
- **THEN** the system writes `keystore.unlock` alongside the active keystore path with a 24-hour TTL
- **AND** subsequent keychain-backed CLI runs and the Agent Skill may read that lease without re-authenticating

#### Scenario: Web lock deletes unlock file
- **WHEN** the user triggers lock from the web interface
- **THEN** the system deletes the `keystore.unlock` file alongside the active keystore path
- **AND** the CLI and web interface both reflect locked state immediately

#### Scenario: Web lock state derived from unlock file
- **GIVEN** no in-memory session state exists
- **WHEN** the web server checks whether the keystore is unlocked
- **THEN** it reads `keystore.unlock` and treats the keystore as unlocked if and only if a valid unexpired lease is found

#### Scenario: CLI unlock is reflected in web interface
- **GIVEN** the user has run `esdiag keystore unlock` from the terminal
- **WHEN** the web interface checks keystore status
- **THEN** it reads the `keystore.unlock` file and shows the keystore as unlocked

#### Scenario: CLI lock is reflected in web interface
- **GIVEN** the web session shows keystore as unlocked
- **AND** the user runs `esdiag keystore lock` from the terminal
- **WHEN** the web interface checks keystore status next
- **THEN** it shows the keystore as locked because the unlock file is gone

### Requirement: Explicit Relock Support
The system SHALL provide an explicit relock action that deletes the `keystore.unlock` file and requires a new secrets password for future keychain-backed actions.

#### Scenario: Relock requested
- **WHEN** the user triggers relock from the web interface
- **THEN** the system deletes the `keystore.unlock` file and marks keychain access as locked for all clients

## REMOVED Requirements

### Requirement: Session-Scoped Unlock Retention
**Reason**: Replaced by file-based unlock (`web-keychain-session-unlock` — File-Based Unlock Shared With CLI). Storing unlock state in memory made the web unlock invisible to the CLI and Agent Skill, requiring users to unlock separately in each context.
**Migration**: No data migration required. After upgrade, the web server reads and writes `keystore.unlock`. Existing CLI unlock leases remain valid.

### Requirement: User Mode Session Lease
**Reason**: The 12-hour in-memory lease is superseded by the file-based unlock lease (24-hour TTL, configurable via CLI `--ttl`). The web server no longer maintains its own session timer.
**Migration**: Unlock leases written by the web server after this change expire after 24 hours by default. Users who previously relied on the 12-hour auto-expiry should use `esdiag keystore lock` or the web lock action explicitly.

### Requirement: Session Lease Refresh on Keystore Access
**Reason**: With a 24-hour file-based lease, refreshing on every keychain read would cause unnecessary write amplification. The lease TTL is long enough to cover any diagnostic session without mid-run refresh.
**Migration**: None. Unlock state persists for the full 24-hour lease duration without refresh.

## ADDED Requirements

### Requirement: Bootstrap Creates Unlock Lease
When the web bootstrap flow creates a new keystore after the user confirms creation and sets a password, the system SHALL immediately write a `keystore.unlock` file so the newly bootstrapped process reflects unlocked state without requiring a separate unlock action.

#### Scenario: Bootstrap writes unlock lease after keystore creation
- **GIVEN** no keystore file exists
- **WHEN** the user completes the web bootstrap modal and a new keystore is created
- **THEN** the system writes `keystore.unlock` alongside the new keystore path with a 24-hour TTL
- **AND** the web interface immediately shows the keystore as unlocked
