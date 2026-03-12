## ADDED Requirements

### Requirement: Secrets Password Unlock for Web Session
The system SHALL require the user to provide the secrets password before any encrypted keychain read or write operation is performed from the web interface.

#### Scenario: Keychain operation attempted while locked
- **WHEN** the user initiates a keychain-backed action and the session is locked
- **THEN** the system prompts for the secrets password and does not perform the keychain operation until unlock succeeds

### Requirement: Session-Scoped Unlock Retention
The system SHALL retain keychain unlock state in server memory for the current user session after a successful unlock until the session is relocked, expired, or terminated.

#### Scenario: Successful unlock retains session state
- **WHEN** the user submits a valid secrets password
- **THEN** subsequent keychain-backed actions in that session execute without prompting again until the session is relocked, expired, or terminated

### Requirement: User Mode Session Lease
In user mode, keystore session awareness SHALL use a 12-hour in-memory session lease for unlocked state tracking.

#### Scenario: Unlock establishes 12-hour lease
- **WHEN** the user successfully unlocks keystore in user mode
- **THEN** the server issues or refreshes a session lease with a 12-hour expiry

### Requirement: Session Lease Refresh on Keystore Access
The system SHALL refresh the session lease on any keystore read and on any request sent to a secure saved host so unlock does not timeout during processing lifecycle.

#### Scenario: Secure host request refreshes lease
- **WHEN** the user sends a request to a secure host while unlocked
- **THEN** the session lease expiry is extended by another 12 hours from that request

### Requirement: Explicit Relock Support
The system SHALL provide an explicit relock action that clears session unlock state and requires a new secrets password for future keychain-backed actions.

#### Scenario: Relock requested
- **WHEN** the user triggers relock from the web interface
- **THEN** the system clears session unlock state and marks keychain access as locked

### Requirement: User Menu Keystore Toggle
The system SHALL provide a `Keystore` menu item in the user pop-up menu that toggles behavior by lock state: selecting it while locked prompts for password, and selecting it while unlocked asks for confirmation before relocking.

#### Scenario: Selecting Keystore while locked
- **WHEN** the user clicks `Keystore` from the user menu and the keystore is locked
- **THEN** the system displays a password prompt for unlock

#### Scenario: Selecting Keystore while unlocked
- **WHEN** the user clicks `Keystore` from the user menu and the keystore is unlocked
- **THEN** the system asks the user to confirm locking and locks only after confirmation

### Requirement: Idempotent Lock Lifecycle Endpoints
The system SHALL expose only `/keystore/unlock` and `/keystore/lock` endpoints for lock lifecycle transitions, and both endpoints SHALL be idempotent.

#### Scenario: Repeated unlock request while unlocked
- **WHEN** the user calls `/keystore/unlock` while already unlocked with a valid password
- **THEN** lock state remains unlocked and the session lease is refreshed

#### Scenario: Repeated lock request while locked
- **WHEN** the user calls `/keystore/lock` while already locked
- **THEN** lock state remains locked and the response is successful

### Requirement: Invalid Password Field Feedback
When unlock submission fails due to incorrect password, the system SHALL keep keystore state locked and mark the password input as invalid so the user can retry.

#### Scenario: Incorrect password on unlock attempt
- **WHEN** the user submits an incorrect secrets password in an unlock prompt
- **THEN** the password field is marked invalid and the user is prompted to re-enter the password

### Requirement: Invalid Password HTTP Semantics
An incorrect unlock password SHALL return HTTP 401 from `/keystore/unlock`.

#### Scenario: Wrong password returns unauthorized
- **WHEN** the unlock password fails to decrypt the keystore
- **THEN** `/keystore/unlock` responds with HTTP 401

### Requirement: Failed Unlock Rate Limiting
Failed unlock attempts SHALL be rate limited in memory with no persistence across process restarts: no delay for first 3 failures, then add 5 minutes per failure from the 4th onward, capped at 60 minutes.

#### Scenario: Backoff begins at fourth failure
- **WHEN** the fourth consecutive unlock failure occurs in a process lifetime
- **THEN** the user is delayed by 5 minutes before another unlock attempt is accepted

#### Scenario: Backoff cap is enforced
- **WHEN** additional failures would exceed the maximum delay
- **THEN** enforced delay is capped at 60 minutes

### Requirement: Keystore Availability Gating
Keystore unlock UI and actions SHALL be available only when the application is built with the `keystore` feature enabled and runtime mode is not `service`.

#### Scenario: Feature-disabled build hides keystore unlock controls
- **WHEN** the application is built without the `keystore` feature
- **THEN** the `Keystore` user-menu item and unlock prompts are not rendered

#### Scenario: Service mode disables keystore unlock controls
- **WHEN** runtime mode is `service`
- **THEN** the `Keystore` user-menu item and unlock prompts are not interactive and are hidden or disabled in the UI

### Requirement: Keystore Route Availability Semantics
When keystore is unavailable (feature disabled or runtime mode `service`), `/keystore/*` routes SHALL not be mounted and requests to those paths SHALL return HTTP 404.

#### Scenario: Unlock route absent when unavailable
- **WHEN** a request is sent to `/keystore/unlock` in feature-disabled or `service` mode
- **THEN** the server responds with HTTP 404

### Requirement: Keystore Status Signal Ownership
The backend SHALL own Datastar status signals `keystore.locked` and `keystore.lock_time` (epoch seconds). These fields are UI status only and SHALL be mutable only through `/keystore/unlock` and `/keystore/lock` responses using JSON PatchSignals payloads.

#### Scenario: Unlock returns PatchSignals update
- **WHEN** `/keystore/unlock` succeeds
- **THEN** the response includes PatchSignals updates for `keystore.locked` and `keystore.lock_time`

#### Scenario: Client cannot set lock status directly
- **WHEN** a client attempts to mutate `keystore.locked` or `keystore.lock_time` in a request body
- **THEN** the server ignores or rejects the mutation and keeps backend state authoritative

### Requirement: Authentication and Timeout Logging
The system SHALL log successful keystore authentications and timeout-based closures as INFO, and failed authentications as WARN.

#### Scenario: Successful unlock logged
- **WHEN** keystore unlock succeeds
- **THEN** an INFO log event is emitted for successful authentication

#### Scenario: Failed unlock logged
- **WHEN** keystore unlock fails due to invalid password
- **THEN** a WARN log event is emitted for failed authentication

#### Scenario: Timeout lock logged
- **WHEN** an unlocked keystore session is closed due to lease timeout
- **THEN** an INFO log event is emitted for timeout closure

### Requirement: Missing Keystore Bootstrap Flow
When keystore storage does not exist, the web UI SHALL prompt the user to create a keystore through the explicit bootstrap modal instead of auto-creating one at process startup.

#### Scenario: Missing keystore opens bootstrap flow
- **WHEN** the application starts in user mode and no keystore file exists
- **THEN** the UI initializes the bootstrap modal flow for explicit keystore creation

#### Scenario: Unlock request falls back to bootstrap flow
- **WHEN** the user requests keystore unlock while no keystore file exists
- **THEN** the system responds with the bootstrap modal rather than auto-creating a keystore
