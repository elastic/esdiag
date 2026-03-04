## MODIFIED Requirements

### Requirement: Datastar Theme Controls
The shared header MUST expose a Datastar-driven control for toggling light/dark mode, and theme mutations MUST be published as Datastar-compatible events on the shared `/events` stream regardless of the underlying stream implementation.

#### Scenario: Toggling light and dark mode
- **WHEN** a user toggles dark mode from the header control
- **THEN** the UI switches between light and dark Borealis token sets.

#### Scenario: Theme toggle response after stream refactor
- **WHEN** the theme toggle endpoint is handled by channel/event-driven streaming internals
- **THEN** the endpoint MAY return `204 No Content`, and the updated theme signal is delivered through `/events` without requiring client protocol changes.
