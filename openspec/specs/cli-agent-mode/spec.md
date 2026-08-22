## Purpose

Define low-noise CLI behavior for agent-driven invocations while preserving the standard structured outcome contract.

## Requirements

### Requirement: Global CLI Agent Mode Activation
The system SHALL provide a parent-level CLI flag `--agent` with short form `-a` that activates agent mode for any top-level CLI command. The system SHALL also activate agent mode automatically when the `CLAUDECODE` environment variable is present.

#### Scenario: User explicitly enables agent mode
- **WHEN** a user runs any `esdiag` CLI command with `--agent`
- **THEN** the command runs in agent mode

#### Scenario: Claude Code auto-enables agent mode
- **WHEN** a user runs any `esdiag` CLI command without `--agent`
- **AND** the `CLAUDECODE` environment variable is present
- **THEN** the command runs in agent mode

### Requirement: Agent Mode Uses Warn-Level Logging By Default
When agent mode is active, the system SHALL use warn-level logging as the default command log level. If `--debug` is also present, the system SHALL continue to use debug-level logging.

#### Scenario: Agent mode suppresses info logging
- **WHEN** a user runs an `esdiag` CLI command in agent mode without `--debug`
- **THEN** the command uses warn-level logging

#### Scenario: Debug flag overrides agent log level
- **WHEN** a user runs an `esdiag` CLI command with both agent mode and `--debug`
- **THEN** the command uses debug-level logging

### Requirement: Agent Mode Preserves The Standard Outcome Contract
Agent mode SHALL use the same typed stdout outcome and selected YAML or JSON format as every other finite CLI invocation. Agent mode MUST NOT introduce a separate summary schema, prose completion message, or output parser requirement.

#### Scenario: Explicit agent mode emits default YAML
- **WHEN** a user runs a finite command with `--agent` and does not specify `--format`
- **THEN** stdout contains the command's standard YAML outcome
- **AND** stderr uses agent mode's warn-level logging default

#### Scenario: Automatic agent mode honors JSON selection
- **GIVEN** `CLAUDECODE` is present
- **WHEN** a user runs a finite command with `--format json`
- **THEN** stdout contains the standard JSON outcome for that command
- **AND** no Claude-specific result shape is used
