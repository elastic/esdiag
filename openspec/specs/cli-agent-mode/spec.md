## Purpose

Define low-noise CLI behavior for agent-driven invocations while preserving explicit completion summaries and normal interactive overrides.

## ADDED Requirements

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

### Requirement: Final CLI Summary Uses STDERR Outside Tracing
The system SHALL write the final human-readable CLI completion summary directly to `STDERR` through an explicit print path instead of relying only on tracing or log-level-controlled output. This behavior SHALL preserve `STDOUT` for streamed command data such as `.ndjson` document output.

#### Scenario: Process command emits final stderr summary
- **WHEN** a processed diagnostic command completes successfully
- **THEN** the CLI writes a final completion summary to `STDERR`
- **AND** the summary includes the final Kibana link when one is available

#### Scenario: Streamed stdout output remains unmodified
- **WHEN** a command streams processed `.ndjson` documents to `STDOUT`
- **THEN** the final human-readable completion summary is written to `STDERR`
- **AND** `STDOUT` contains only the streamed document output

#### Scenario: Non-process command emits one final stderr result
- **WHEN** a non-process CLI command completes successfully
- **THEN** the CLI writes a concise final success result to `STDERR`
