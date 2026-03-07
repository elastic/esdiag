## MODIFIED Requirements

### Requirement: Settings Modal Access
The web UI SHALL provide footer controls where the output target selector remains available for switching active targets, and host record management is launched through a dedicated `Edit Hosts` action rather than by clicking the target text.

#### Scenario: Opening host manager from footer
- **GIVEN** the web UI is loaded
- **WHEN** the user clicks the `Edit Hosts` button in the footer controls
- **THEN** a host/keychain manager modal opens

### Requirement: Output Target Selection
The footer output selector SHALL only allow selecting an existing `KnownHost` and saving that selection as the active target; it SHALL NOT allow inline creation of a new host entry.

#### Scenario: Selecting an existing host as output target
- **GIVEN** the footer output selector is visible
- **WHEN** the user chooses an existing host name and clicks save
- **THEN** the backend updates `settings.yml` to set the active target to that host's name

#### Scenario: Inline host creation is unavailable in output selector
- **GIVEN** the footer output selector is open
- **WHEN** the user inspects available controls
- **THEN** no add-new-host form fields are present in that selector flow

## ADDED Requirements

### Requirement: Footer Edit Hosts Action Placement
The footer SHALL place an `Edit Hosts` action adjacent to the save control used for output target persistence so users can discover host/keychain management without leaving the current screen.

#### Scenario: Edit action appears next to save
- **GIVEN** the footer configuration controls are rendered
- **WHEN** the user views the output target save area
- **THEN** an `Edit Hosts` action is displayed next to the save control
