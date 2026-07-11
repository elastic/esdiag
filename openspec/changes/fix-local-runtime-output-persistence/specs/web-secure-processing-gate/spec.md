## MODIFIED Requirements

### Requirement: Processing Gate Availability by Feature and Mode
The secure-host unlock preflight prompt SHALL run only when the `keystore` feature is enabled, runtime mode is not `service`, and the active output depends on keystore-backed local secrets. An active exporter whose authentication is already fully provided by runtime configuration SHALL proceed without keystore bootstrap or unlock in any runtime mode.

#### Scenario: Runtime-configured secure output proceeds when feature disabled
- **WHEN** the application is built without the `keystore` feature and a processing action targets an exporter whose authentication was already provided by runtime configuration instead of local keystore storage
- **THEN** processing starts without an unlock prompt because there is no local keystore dependency to satisfy

#### Scenario: Runtime-configured secure output proceeds in service mode
- **WHEN** runtime mode is `service` and a processing action targets an exporter whose authentication was already provided by runtime configuration instead of local keystore storage
- **THEN** processing starts without an unlock prompt because keystore interaction is not required

#### Scenario: Runtime-configured secure output proceeds in user mode
- **WHEN** runtime mode is `user` and a processing action targets an exporter whose authentication was already provided by runtime configuration instead of local keystore storage
- **THEN** processing starts without creating or unlocking a keystore
- **AND** runtime authentication is not persisted into local keystore state

#### Scenario: Keystore-backed secure output still requires unlock when available
- **WHEN** the active output depends on local keystore-backed secrets and keystore capability is available
- **THEN** processing does not start until unlock succeeds
