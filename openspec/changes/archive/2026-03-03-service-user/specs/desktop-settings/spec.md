## MODIFIED Requirements

### Requirement: Persistent Desktop Settings
The system SHALL support mode-aware settings persistence. In `user` mode, it SHALL read and write configuration settings (active exporter and Kibana URL) to a local settings file alongside `hosts.yml`. In `service` mode, it SHALL avoid local credential and host persistence and only retain limited, non-sensitive preferences.

#### Scenario: User mode persists local settings
- **GIVEN** the web interface is running in `user` mode
- **WHEN** the user configures a custom target host and restarts the application without CLI arguments
- **THEN** the server initializes using persisted local settings and host target data

#### Scenario: Service mode does not persist local credentials
- **GIVEN** the web interface is running in `service` mode
- **WHEN** a user updates available preferences from the UI
- **THEN** the system does not write credentials or host target records to local `settings.yml` or `hosts.yml` artifacts

### Requirement: Output Target Selection
The settings UI SHALL provide mode-aware target behavior. In `user` mode, the modal SHALL allow selecting an existing `KnownHost` from `hosts.yml` or manually inputting a new host (URL, API Key, Username, Password). In `service` mode, host creation and persisted credential flows SHALL be unavailable, and export target selection SHALL be constrained to the startup-defined exporter.

#### Scenario: Selecting an existing host in user mode
- **GIVEN** the settings modal is open in `user` mode
- **WHEN** the user selects an existing host from a dropdown and submits
- **THEN** the backend updates persisted settings to set the active target to that host name

#### Scenario: Service mode enforces fixed exporter
- **GIVEN** the settings modal is open in `service` mode
- **WHEN** the user attempts to change host-backed exporter credentials
- **THEN** the UI blocks persisted host credential updates and the backend continues using the startup-defined exporter

### Requirement: Dynamic State Updates
Updating configuration via the settings surface SHALL update `ServerState` dynamically. In `user` mode, runtime updates SHALL include exporter and host-backed preferences without restart. In `service` mode, runtime updates SHALL be limited to allowed preferences and MUST NOT change the fixed exporter contract.

#### Scenario: Server uses new target immediately in user mode
- **GIVEN** the server is running in `user` mode and targeting "Host A"
- **WHEN** the user updates the target to "Host B" via the settings modal
- **THEN** subsequent uploads processed by the backend are exported to "Host B" without requiring a server restart

#### Scenario: Service mode rejects runtime exporter replacement
- **GIVEN** the server is running in `service` mode with a startup-defined exporter
- **WHEN** a runtime settings update attempts to switch exporter target
- **THEN** the request is rejected or ignored according to policy and the startup exporter remains active
