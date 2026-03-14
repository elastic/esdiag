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
The secure-host unlock preflight prompt SHALL run only when the `keystore` feature is enabled, runtime mode is not `service`, and the active output depends on keystore-backed local secrets. When keystore capability is unavailable but the active exporter is already fully configured by runtime-provided authentication outside local keystore storage, processing MAY proceed without unlock.

#### Scenario: Runtime-configured secure output proceeds when feature disabled
- **WHEN** the application is built without the `keystore` feature and a processing action targets an exporter whose authentication was already provided by runtime configuration instead of local keystore storage
- **THEN** processing starts without an unlock prompt because there is no local keystore dependency to satisfy

#### Scenario: Runtime-configured secure output proceeds in service mode
- **WHEN** runtime mode is `service` and a processing action targets an exporter whose authentication was already provided by runtime configuration instead of local keystore storage
- **THEN** processing starts without an unlock prompt because keystore interaction is not required

#### Scenario: Keystore-backed secure output still requires unlock when available
- **WHEN** the active output depends on local keystore-backed secrets and keystore capability is available
- **THEN** processing does not start until unlock succeeds
