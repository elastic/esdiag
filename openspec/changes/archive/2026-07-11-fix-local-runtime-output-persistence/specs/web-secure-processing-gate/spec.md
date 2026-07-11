## MODIFIED Requirements

### Requirement: Processing Gate Availability by Feature and Mode
The secure-host unlock preflight prompt SHALL run only when the `keystore` feature is enabled, runtime mode is not `service`, and the UI explicitly selects a saved output that depends on keystore-backed local secrets. When the UI leaves output unset, the environment-backed exporter fallback SHALL proceed without keystore bootstrap or unlock.

#### Scenario: Runtime-configured secure output proceeds when feature disabled
- **WHEN** the application is built without the `keystore` feature and a processing action targets an exporter whose authentication was already provided by runtime configuration instead of local keystore storage
- **THEN** processing starts without an unlock prompt because there is no local keystore dependency to satisfy

#### Scenario: Runtime-configured secure output proceeds in service mode
- **WHEN** runtime mode is `service` and a processing action targets an exporter whose authentication was already provided by runtime configuration instead of local keystore storage
- **THEN** processing starts without an unlock prompt because keystore interaction is not required

#### Scenario: Unset UI output proceeds in user mode
- **WHEN** runtime mode is `user`, the UI does not select a saved output, and runtime output configuration supplies the exporter
- **THEN** processing starts without creating or unlocking a keystore
- **AND** runtime authentication is not persisted into local keystore state

#### Scenario: Matching saved-host URL does not imply selection
- **GIVEN** an environment-backed exporter has the same URL as a saved keystore-backed host
- **AND** the UI has not selected that saved host
- **WHEN** processing preflight runs
- **THEN** processing does not require the saved host's keystore secret

#### Scenario: Keystore-backed secure output still requires unlock when available
- **WHEN** the active output depends on local keystore-backed secrets and keystore capability is available
- **THEN** processing does not start until unlock succeeds
