## Purpose

Define the dedicated web management UI for saved hosts and keychain-linked authentication metadata.

## Requirements

### Requirement: KnownHost Record Editing UI
The system SHALL provide a Datastar-powered management UI that allows users to create, update, and delete `KnownHost` records stored in `hosts.yml`, including all persisted fields required by the `KnownHost` model.

#### Scenario: Updating a host record
- **WHEN** the user edits a host record in the management UI and submits valid changes
- **THEN** the backend persists the updated record to `hosts.yml` and returns refreshed host metadata to the UI

### Requirement: Keychain-Referenced Authentication Selection
The host manager SHALL allow selecting authentication from keychain entry names, and SHALL persist only the selected keychain reference in the host record.

#### Scenario: Assigning keychain auth reference
- **WHEN** the user selects a keychain entry name for host authentication and saves the host
- **THEN** the host record stores the keychain reference and does not embed secret values in `hosts.yml`

### Requirement: Backend-Only Secret Material Exposure
The system MUST ensure frontend responses and Datastar state updates include keychain entry metadata only (for example, entry names) and MUST NOT include decrypted secret values from persisted keychain storage. Transient plaintext values that the user is actively editing MAY exist in draft state only for submission back to the backend.

#### Scenario: Loading keychain list in management UI
- **WHEN** the user opens the keychain section of the management UI
- **THEN** the frontend receives a list of keychain entry names and metadata without any secret payload values

#### Scenario: Editing a draft secret
- **WHEN** the user enters a plaintext secret while editing an active draft
- **THEN** that plaintext value may exist in transient draft state until the save action is submitted or canceled

### Requirement: Host Validation Before Persistence
The backend SHALL validate host fields and keychain reference existence before persisting changes from the management UI.

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

### Requirement: Web Host Classification Parity
The Web host manager SHALL use the same application, endpoint-state, route, template
resolution, and role validation rules as the CLI. The Web form MUST offer only actual
Stack applications as application values and MUST NOT expose a platform, Cloud admin
route, or `Unknown` as an application choice.

#### Scenario: Create concrete Cloud admin host
- **GIVEN** a user enters a recognized Cloud admin proxy URL for Elasticsearch
- **WHEN** the user saves the host form
- **THEN** the backend classifies the target application as Elasticsearch
- **AND** preserves Cloud admin as route metadata
- **AND** returns the normalized host record to the UI

#### Scenario: Platform is absent from application choices
- **WHEN** the Web host manager renders the application selector
- **THEN** ECK, ECE, Elastic Cloud Hosted, Kubernetes Platform, and Unknown are not application choices

#### Scenario: Backend rejects forged platform application
- **GIVEN** a request bypasses the rendered application selector
- **WHEN** it submits a platform value as the host application
- **THEN** backend validation rejects the request
- **AND** persisted host storage remains unchanged

### Requirement: Web Template Resolution State
The Web host manager SHALL distinguish an unresolved template-backed record from a
concrete classified host. It SHALL render an unresolved template without a fake
application and SHALL require successful materialization before any connection or
authentication test that needs a concrete endpoint.

#### Scenario: Display unresolved dynamic template
- **GIVEN** a saved dynamic template has no application before materialization
- **WHEN** the Web host manager renders the record
- **THEN** the UI identifies the application as unresolved
- **AND** does not display `Unknown` or a platform as the application

#### Scenario: Test unresolved template
- **GIVEN** a dynamic template has not been supplied an identifier and application
- **WHEN** the user requests an authentication or connection test
- **THEN** the backend rejects the test with template-resolution guidance

#### Scenario: Materialize template from Web workflow
- **GIVEN** a Web workflow selects a saved dynamic template
- **WHEN** the user supplies an identifier and supported application
- **THEN** the backend renders and validates a resolved host
- **AND** only the resolved host is passed to runtime dispatch
