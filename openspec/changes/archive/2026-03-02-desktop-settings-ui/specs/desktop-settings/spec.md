## ADDED Requirements

### Requirement: Persistent Desktop Settings
The system SHALL support reading and writing configuration settings (active Exporter and Kibana URL) to a `settings.yml` file located alongside the existing `hosts.yml` file.

#### Scenario: Settings are persisted across sessions
- **GIVEN** the user has configured a custom target host via the UI
- **WHEN** the application is restarted without CLI arguments
- **THEN** the server initializes using the host target and Kibana URL read from `settings.yml`

### Requirement: Settings Modal Access
The web UI SHALL provide an interactable element in the footer displaying the currently active target. Clicking this element SHALL open a configuration modal.

#### Scenario: Opening the settings modal
- **GIVEN** the web UI is loaded
- **WHEN** the user clicks on the "Target: [CurrentTarget]" text in the footer
- **THEN** a modal opens displaying inputs for Kibana URL and Output Target selection

### Requirement: Output Target Selection
The modal SHALL allow the user to select an existing `KnownHost` from their `hosts.yml` file or manually input details for a new host (URL, API Key, Username, Password).

#### Scenario: Selecting an existing host
- **GIVEN** the settings modal is open
- **WHEN** the user selects an existing host from a dropdown and submits
- **THEN** the backend updates `settings.yml` to set the active target to that host's name

#### Scenario: Creating a new host
- **GIVEN** the settings modal is open
- **WHEN** the user provides details for a new host and clicks save
- **THEN** the backend validates the connection, adds the host to `hosts.yml`, and sets it as the active target in `settings.yml`

### Requirement: Dynamic State Updates
Updating the configuration via the settings modal SHALL update the `ServerState` dynamically without requiring the process to be terminated.

#### Scenario: Server uses new target immediately
- **GIVEN** the server is running and targeting "Host A"
- **WHEN** the user updates the target to "Host B" via the settings modal
- **THEN** subsequent uploads processed by the backend are exported to "Host B" without needing a server restart

### Requirement: Secret Redaction in UI
All credential input fields (API Key, Passwords) in the settings modal SHALL visually obscure their contents to prevent accidental shoulder-surfing leaks.

#### Scenario: Viewing the credentials input
- **GIVEN** the user is viewing the new host form
- **WHEN** they type an API Key or Password
- **THEN** the characters are visually hidden (e.g., using `type="password"`)
