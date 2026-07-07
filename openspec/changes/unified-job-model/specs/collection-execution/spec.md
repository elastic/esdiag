## ADDED Requirements

### Requirement: Phase-Composed Job
The system SHALL model one diagnostic execution as a `Job` that selects stages within three
ordered phases: **Phase 1 (input, required)** is `Collect` xor `Load`; **Phase 2 (middle,
optional)** is `Save` and/or `Process`; **Phase 3 (output, optional)** is `Export` and/or
`Send`. `Export` SHALL live inside `Process` (`Process { selection, export }`) so that a
`Process` stage always has an export sink and an export can exist only with processing. The
`Job` constructor SHALL enforce the dependency invariants and reject any job that violates
them: `Save` requires `Collect` input; `Send` requires that a bundle exists (`Load` input or
`Save` set); and at least one of `Save`, `Process`, or `Send` MUST be selected.

#### Scenario: Save without collect is rejected
- **WHEN** a `Job` is constructed with `Load` input and a `Save` stage
- **THEN** construction MUST fail because `Save` requires `Collect` input

#### Scenario: Send without a bundle is rejected
- **WHEN** a `Job` is constructed with `Collect` input, a `Process` stage, no `Save`, and a `Send` stage
- **THEN** construction MUST fail because `Send` requires an existing bundle (`Load` input or `Save`)

#### Scenario: A job must do something
- **WHEN** a `Job` is constructed with a `Collect` input and no `Save`, `Process`, or `Send`
- **THEN** construction MUST fail because the job selects no Phase-2 or Phase-3 stage

#### Scenario: Export cannot exist without process
- **WHEN** a `Job` is expressed with an export target but no processing
- **THEN** the model MUST make that state unrepresentable because `Export` lives inside `Process`

### Requirement: Derived Execution Mode
The system SHALL derive a job's execution mode from its stage selection rather than storing
it. A job that selects both `Save` and `Process` SHALL execute in **staged** mode, where
collection completes and the bundle materialises before processing reads it (the bundle is a
serialization barrier). A job that selects `Collect` and `Process` without `Save` SHALL
execute in **streaming** mode, where receive, transform, and export overlap concurrently. A
single executor SHALL drive both modes.

#### Scenario: Save plus process is staged
- **WHEN** a job selects `Collect`, `Save`, and `Process`
- **THEN** the executor MUST complete collection and materialise the bundle before processing begins

#### Scenario: Collect plus process without save is streaming
- **WHEN** a job selects `Collect` and `Process` with no `Save`
- **THEN** the executor MUST overlap receiving, transforming, and exporting concurrently
- **AND** MUST NOT require an intermediate bundle to materialise first

#### Scenario: One executor drives both modes
- **WHEN** either a staged job or a streaming job is executed
- **THEN** both MUST run through the same executor, which selects its strategy from the derived mode

### Requirement: Load Input Jobs
The system SHALL support jobs whose Phase-1 input is `Load` — reading an existing diagnostic
from a directory or bundle — in place of `Collect`. A `Load`-input job MAY select `Process`
and/or `Send` (a bundle already exists) but MUST NOT select `Save`.

#### Scenario: Load then process
- **WHEN** a job is configured with `Load` input over an existing bundle and a `Process` stage
- **THEN** the executor MUST read the loaded bundle as its input and produce processed documents
- **AND** MUST NOT perform any live collection

#### Scenario: Load then send
- **WHEN** a job is configured with `Load` input and a `Send` stage and no `Process`
- **THEN** the executor MUST transmit the loaded bundle without producing processed documents

### Requirement: Concurrent Export and Send
The system SHALL allow a single job to select both `Export` (processed documents) and `Send`
(the raw bundle) in Phase 3, executing both outputs in one run. `Export` and `Send` are
independent and SHALL NOT be mutually exclusive when their preconditions are met.

#### Scenario: Save, process, export, and send in one run
- **WHEN** a job selects `Collect`, `Save`, `Process` (with an export sink), and `Send`
- **THEN** the executor MUST index the processed documents to the export destination
- **AND** MUST also transmit the saved raw bundle via `Send` in the same run

## MODIFIED Requirements

### Requirement: Collect-Without-Process Workflow
The system SHALL support transmitting a diagnostic bundle without invoking processing when a
job selects a `Send` stage and no `Process` stage. The bundle MAY originate from `Collect` +
`Save` this run or from a `Load` input.

#### Scenario: Collect and save then send without processing
- **GIVEN** a job configured with `Collect`, `Save`, and `Send` and no `Process`
- **WHEN** the job runs
- **THEN** the system completes collection and materialises the bundle without creating processed diagnostic documents
- **AND** the `Send` stage transmits that saved bundle

#### Scenario: Load then send without processing
- **GIVEN** a job configured with `Load` input and a `Send` stage and no `Process`
- **WHEN** the job runs
- **THEN** the system transmits the loaded bundle without creating processed diagnostic documents

## REMOVED Requirements

### Requirement: One-Job and Two-Job Workflow Modes
**Reason**: The one-/two-job boundary was an artifact of the always-staged legacy path.
Under the unified model a job is a single `Job` whose execution mode (staged vs streaming) is
*derived* from whether `Save` is selected — covered by the new "Derived Execution Mode"
requirement. There is no second job created when saving.
**Migration**: `Collect -> Process -> Send` without `Save` is now one **streaming** job;
`Collect -> Save -> Process` (optionally with `Send`) is now one **staged** job. Callers that
previously created a second job to consume the retained archive instead construct a single
staged `Job`; the executor materialises the bundle as the serialization barrier internally.
