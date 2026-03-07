## ADDED Requirements

### Requirement: KnownHost Record Editing Modal
The system SHALL provide a Datastar-powered modal that allows users to create, update, and delete `KnownHost` records stored in `hosts.yml`, including all persisted fields required by the `KnownHost` model.

#### Scenario: Updating a host record
- **WHEN** the user edits a host record in the manager modal and submits valid changes
- **THEN** the backend persists the updated record to `hosts.yml` and returns refreshed host metadata to the UI

### Requirement: Keychain-Referenced Authentication Selection
The host manager SHALL allow selecting authentication from keychain entry names, and SHALL persist only the selected keychain reference in the host record.

#### Scenario: Assigning keychain auth reference
- **WHEN** the user selects a keychain entry name for host authentication and saves the host
- **THEN** the host record stores the keychain reference and does not embed secret values in `hosts.yml`

### Requirement: Backend-Only Secret Material Exposure
The system MUST ensure frontend responses and Datastar state updates include keychain entry metadata only (for example, entry names) and MUST NOT include decrypted secret values.

#### Scenario: Loading keychain list in manager modal
- **WHEN** the user opens the keychain section of the manager modal
- **THEN** the frontend receives a list of keychain entry names and metadata without any secret payload values

### Requirement: Host Validation Before Persistence
The backend SHALL validate host fields and keychain reference existence before persisting changes from the manager modal.

#### Scenario: Save rejected for invalid keychain reference
- **WHEN** the user submits a host referencing a non-existent keychain entry
- **THEN** the system rejects the save, leaves persisted data unchanged, and returns a validation error to the UI

### Requirement: Visible Keystore Lock Status
The keystore manager view SHALL display a lock-status glyph/icon indicating whether the current session is locked or unlocked for keystore use.

#### Scenario: Manager page reflects unlocked state
- **WHEN** the user opens or refreshes the keystore manager while keystore session state is unlocked
- **THEN** the UI shows the unlocked glyph/icon

#### Scenario: Manager page reflects locked state
- **WHEN** keystore session state is locked
- **THEN** the UI shows the locked glyph/icon

### Requirement: Manager Keystore UI Availability
The keystore-specific portions of the manager UI (including lock glyph and keychain secret-binding controls) SHALL be available only when the application is built with the `keystore` feature and runtime mode is not `service`.

#### Scenario: Manager keystore controls absent when feature is disabled
- **WHEN** the application is built without the `keystore` feature
- **THEN** the manager does not render keystore-specific controls

#### Scenario: Manager keystore controls disabled in service mode
- **WHEN** runtime mode is `service`
- **THEN** the manager does not allow keystore-specific interactions
