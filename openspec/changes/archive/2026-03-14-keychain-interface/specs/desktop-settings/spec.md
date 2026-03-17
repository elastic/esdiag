## MODIFIED Requirements

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
- **THEN** the backend updates `settings.yml` to set the active target to that host's name

#### Scenario: CLI-defined output is preserved as selected target
- **GIVEN** the application started with a CLI-defined output target that is not a saved host
- **WHEN** the footer output selector is rendered
- **THEN** the live output target appears as an option with an exporter-type label and remains selected by default

#### Scenario: Inline host creation is unavailable in output selector
- **GIVEN** the footer output selector is open
- **WHEN** the user inspects available controls
- **THEN** no add-new-host form fields are present in that selector flow
