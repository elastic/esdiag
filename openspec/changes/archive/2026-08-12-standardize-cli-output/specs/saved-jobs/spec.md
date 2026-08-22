## MODIFIED Requirements

### Requirement: CLI Job Listing
The system SHALL provide `esdiag job list` as a CLI subcommand that emits a structured saved-job list from `~/.esdiag/jobs.yml`. Each job entry SHALL include its name, `Collect` or `Load` input, optional `Save`, optional `Process` with its export target, and optional `Send` configuration. It MUST NOT synthesize the retired mutually exclusive action field or expose credential material. CLI job listing SHALL NOT depend on `ServerPolicy`, runtime mode, or `ESDIAG_WEB_FEATURES`.

#### Scenario: List saved jobs
- **WHEN** the user runs `esdiag job list` and `jobs.yml` contains entries
- **THEN** the system emits a YAML job-list outcome by default and exits with code 0
- **AND** each saved job appears as one typed entry
- **AND** no terminal table is present
- **AND** non-interactive stdout contains no ANSI color

#### Scenario: List with no saved jobs
- **WHEN** the user runs `esdiag job list` and `jobs.yml` does not exist or is empty
- **THEN** the system emits a successful job-list outcome containing `jobs: []`
- **AND** exits with code 0

#### Scenario: Web feature flags do not affect CLI listing
- **GIVEN** `ESDIAG_WEB_FEATURES` is set to an empty string
- **WHEN** the user runs `esdiag job list`
- **THEN** CLI listing behavior and its structured result are unchanged

## ADDED Requirements

### Requirement: CLI Saved Job Operations Return Typed Outcomes
Saved-job run and delete operations SHALL return typed CLI outcomes. A successful finite run SHALL return one composite job result containing every durable result produced by its selected `Save`, `Process`, and `Send` stages; deletion SHALL identify the removed job by name without returning its persisted configuration or any credential material. The job result SHALL reuse the same bundle, process, and send projection types used by direct CLI operations.

#### Scenario: Save-only job reports retained archive
- **WHEN** `esdiag job run <name>` completes a `Collect` plus retained `Save` job without processing or sending
- **THEN** its structured result contains a save result with the resolved archive path
- **AND** contains no process or send result

#### Scenario: Send-only load job reports destination
- **WHEN** `esdiag job run <name>` completes a `Load` plus `Send` job
- **THEN** its structured result contains the resolved upload destination
- **AND** does not mislabel the loaded input path as an archive created by `Save`

#### Scenario: Temporary save is not reported as retained
- **WHEN** a job uses a temporary `Save` serialization barrier before `Send`
- **THEN** its structured result contains the send destination
- **AND** omits a save result because no archive survives the run

#### Scenario: Process job reports diagnostic
- **WHEN** `esdiag job run <name>` completes a `Process` stage
- **THEN** its structured result contains the primary diagnostic result and all included diagnostic outcomes

#### Scenario: One job reports save, process, and send
- **WHEN** a job successfully collects, retains its bundle, processes and exports documents, and sends the raw bundle
- **THEN** one `job_completed` result contains save, process, and send results
- **AND** no result is discarded in favor of a primary variant

#### Scenario: Delete reports affected job
- **WHEN** `esdiag job delete <name>` succeeds
- **THEN** its structured result identifies `<name>` as deleted
- **AND** does not expose the removed job's credential references beyond safe identifiers

### Requirement: Failed Jobs Preserve Completed Stage Facts
When a finite saved-job run fails after one or more earlier stages created durable results, its structured failure SHALL identify the failed stage and include allowlisted completed-stage facts from the accumulated executor outcome. The command MUST still exit non-zero, MUST NOT label the job completed, and MUST indicate when retrying the whole job is unsafe.

#### Scenario: Send fails after save and process complete
- **GIVEN** a job retained an archive and processed a diagnostic successfully
- **WHEN** its later `Send` stage fails
- **THEN** stdout contains a structured non-zero failure identifying `send` as the failed stage
- **AND** includes the retained archive path and processed diagnostic result
- **AND** marks a whole-job retry unsafe

#### Scenario: Process fails after save completes
- **GIVEN** a job retained a newly collected archive successfully
- **WHEN** its `Process` stage fails
- **THEN** the structured failure includes the retained archive path and identifies `process` as failed
- **AND** does not fabricate a process or send result
