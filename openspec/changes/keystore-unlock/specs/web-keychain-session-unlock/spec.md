## MODIFIED Requirements

### Requirement: Session-Scoped Unlock Retention
In user mode, the system SHALL retain keychain unlock state as a single local in-memory web session after a successful unlock until the session is relocked, expired, or terminated. User mode MAY initialize that in-memory session from a valid existing CLI unlock lease, but the web runtime SHALL NOT create, refresh, extend, or otherwise write the CLI unlock file as part of normal session lifecycle. Readers MAY still perform best-effort deletion of expired or otherwise stale CLI unlock files encountered while reading them. Service mode SHALL NOT expose or maintain keystore session state and SHALL ignore CLI unlock files.

#### Scenario: Successful unlock retains session state
- **WHEN** the user submits a valid secrets password
- **THEN** subsequent keychain-backed actions in that session execute without prompting again until the session is relocked, expired, or terminated

#### Scenario: Existing CLI unlock lease seeds web session
- **GIVEN** the application is running in user mode
- **AND** the in-memory web session is still locked
- **AND** a valid CLI unlock lease already exists
- **WHEN** the web runtime first checks whether keystore access is available
- **THEN** the system initializes the in-memory web session as unlocked from that lease
- **AND** the web runtime does not create, refresh, extend, or rewrite the CLI unlock file

#### Scenario: Explicit relock does not delete CLI unlock file
- **GIVEN** the user-mode web session was initialized from a valid CLI unlock lease
- **WHEN** the user triggers relock from the web interface
- **THEN** the system clears only the in-memory web session unlock state
- **AND** the CLI unlock file remains unchanged
- **AND** the same running web process does not immediately reseed itself from that file again
