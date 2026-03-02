## ADDED Requirements

### Requirement: Global Theme Model
The Web UI MUST provide a global theme model that supports at least light and dark modes across all primary pages and shared components.

#### Scenario: Rendering any primary UI route
- **WHEN** a user loads a primary Web UI page (including docs, index/upload flow, and shared layout pages)
- **THEN** the page renders using the active global theme tokens.

### Requirement: Theme Preference Persistence
The system MUST persist a user light/dark preference so the selected mode survives route transitions and page reloads.

#### Scenario: Returning to the UI after toggling mode
- **WHEN** a user toggles light/dark mode, then navigates to another route or reloads the page
- **THEN** the previously selected mode remains active.

### Requirement: Datastar Theme Controls
The shared header MUST expose a Datastar-driven control for toggling light/dark mode.

#### Scenario: Toggling light and dark mode
- **WHEN** a user toggles dark mode from the header control
- **THEN** the UI switches between light and dark Borealis token sets.

### Requirement: Theme Asset Organization
Theme implementation MUST separate shared structural styles from Borealis token definitions so light/dark mode can be maintained without duplicating layout/component CSS.

#### Scenario: Loading themed UI assets
- **WHEN** a themed UI page is rendered
- **THEN** the system applies Borealis token stylesheet(s) together with shared base styles.

### Requirement: Offline Theme Operation
Theme behavior MUST remain fully functional in offline/air-gapped environments.

#### Scenario: Running without external network access
- **WHEN** the Web UI is used in an environment without internet access
- **THEN** theme controls and themed rendering continue to work using local embedded assets only.
