## MODIFIED Requirements

### Requirement: Saved Host Listing
The system SHALL provide `esdiag host list` to emit a structured list outcome containing saved host entries. Each entry SHALL include `name`, `app`, roles, URL or template metadata safe for display, and the saved secret reference identifier when present. It MUST NOT include resolved API keys or passwords. When no hosts are saved, the outcome SHALL contain an empty `hosts` sequence.

#### Scenario: List emits structured hosts
- **GIVEN** saved hosts exist in persisted host storage
- **WHEN** the user runs `esdiag host list`
- **THEN** the command emits a YAML host-list outcome by default
- **AND** each saved host appears as one typed entry in `hosts`
- **AND** no terminal table is present
- **AND** non-interactive stdout contains no ANSI color

#### Scenario: List represents empty host storage
- **GIVEN** no saved hosts exist
- **WHEN** the user runs `esdiag host list`
- **THEN** the command emits a successful host-list outcome with `hosts: []`
- **AND** it does not emit an empty-state sentence that callers must parse
