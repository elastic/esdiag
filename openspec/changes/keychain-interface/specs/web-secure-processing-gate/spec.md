## ADDED Requirements

### Requirement: Secure Host Processing Requires Unlocked Keystore
The system SHALL block starting diagnostic processing for secure hosts that depend on keychain-backed secrets when the keystore is locked.

#### Scenario: Processing attempt while keystore locked
- **WHEN** the user attempts to start processing for a secure host and the keystore is locked
- **THEN** processing does not start and the user is prompted for the keystore password in a modal

### Requirement: Secure Host Classification
Host security classification SHALL be derived from authentication type, where only `NoAuth` is non-secure and all other auth types are secure.

#### Scenario: NoAuth host bypasses keystore preflight
- **WHEN** processing starts for a host configured with `NoAuth`
- **THEN** keystore unlock preflight is not required

### Requirement: Unlock-Then-Proceed Processing Flow
If the user provides the correct keystore password in the processing preflight prompt, the system SHALL unlock the keystore and continue starting processing without requiring the user to re-trigger the action.

#### Scenario: Correct password in processing preflight
- **WHEN** the user submits a correct password from the secure-host processing prompt
- **THEN** keystore state transitions to unlocked and processing starts for the requested action

### Requirement: Processing Lifecycle Session Refresh
During secure-host processing lifecycle, each keystore-backed host request SHALL refresh the active session lease to prevent timeout during processing.

#### Scenario: Long-running processing does not timeout mid-run
- **WHEN** secure-host processing performs multiple keystore-backed requests over time
- **THEN** each request refreshes session lease and unlock state remains valid for the active lifecycle

### Requirement: Incorrect Password Blocks Processing With Field Error
If the user provides an incorrect password in the processing preflight prompt, the system SHALL keep processing blocked and mark the password input invalid.

#### Scenario: Incorrect password in processing preflight
- **WHEN** the user submits an incorrect password from the secure-host processing prompt
- **THEN** processing remains blocked, the password input is invalidated, and the user is prompted again

### Requirement: Processing Gate Availability by Feature and Mode
The secure-host unlock preflight prompt SHALL run only when the `keystore` feature is enabled and runtime mode is not `service`; otherwise, secure-host processing start SHALL be rejected with a keystore-unavailable error.

#### Scenario: Secure-host start rejected when feature disabled
- **WHEN** the application is built without the `keystore` feature and a secure-host processing action is started
- **THEN** processing does not start and the system returns a keystore-unavailable error

#### Scenario: Secure-host start rejected in service mode
- **WHEN** runtime mode is `service` and a secure-host processing action is started
- **THEN** processing does not start and the system returns a keystore-unavailable error
