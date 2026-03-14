## ADDED Requirements

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

### Requirement: Settings Modal Access
The web UI SHALL provide footer controls where the output target selector remains available for switching active targets, while host and secret management live in the dedicated `/settings` interface.

#### Scenario: Opening host manager from navigation
- **GIVEN** the web UI is loaded
- **WHEN** the user selects `Settings`
- **THEN** the application opens the dedicated host/keychain management interface

### Requirement: Output Target Selection
The footer output selector SHALL allow selecting an existing saved `KnownHost`, and it SHALL also surface the live CLI-defined output target when it is not one of the saved hosts. It SHALL NOT allow inline creation of a new host entry.

#### Scenario: Selecting an existing host as output target
- **GIVEN** the footer output selector is visible
- **WHEN** the user chooses an existing host name
- **THEN** the backend updates persisted settings to set the active target to that host name

#### Scenario: CLI-defined output is preserved as selected target
- **GIVEN** the application started with a CLI-defined output target that is not a saved host
- **WHEN** the footer output selector is rendered
- **THEN** the live output target appears as an option with an exporter-type label and remains selected by default

#### Scenario: Inline host creation is unavailable in output selector
- **GIVEN** the footer output selector is open
- **WHEN** the user inspects available controls
- **THEN** no add-new-host form fields are present in that selector flow

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

### Requirement: Secret Redaction in UI
All credential input fields (API Key, Passwords) in the settings modal SHALL visually obscure their contents to prevent accidental shoulder-surfing leaks.

#### Scenario: Viewing the credentials input
- **GIVEN** the user is viewing the new host form
- **WHEN** they type an API Key or Password
- **THEN** the characters are visually hidden (e.g., using `type="password"`)
