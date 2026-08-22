## ADDED Requirements

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

## REMOVED Requirements

### Requirement: Final CLI Summary Uses STDERR Outside Tracing
**Reason**: Human-readable stderr summaries force agents and applications to scrape prose and conflict with the new typed stdout outcome contract. Stderr remains reserved for tracing, warnings, and progress.

**Migration**: Deserialize the command's YAML stdout by default, or request JSON with `--format json`. Commands that intentionally stream NDJSON continue to reserve stdout for that stream and emit no differently shaped terminal summary.
