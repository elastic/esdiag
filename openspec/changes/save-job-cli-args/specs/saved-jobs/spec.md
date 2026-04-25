## ADDED Requirements

### Requirement: CLI Invocation-Derived Job Save

The system SHALL allow compatible `esdiag collect` and `esdiag process` invocations to persist a named job by accepting `--save-job <name>`. The job SHALL be derived from the effective command invocation and persisted to `~/.esdiag/jobs.yml` using the same job validation rules as other persistence paths.

#### Scenario: Collect command saves a compatible job

- **WHEN** the user runs `esdiag collect --save-job my-job [ARGS]` with a valid known-host collection invocation
- **THEN** the system persists `my-job` to `~/.esdiag/jobs.yml`
- **AND** the command continues using the unchanged collect execution arguments

#### Scenario: Process command saves a compatible job

- **WHEN** the user runs `esdiag process --save-job my-job [ARGS]` with a valid saved-job-compatible invocation
- **THEN** the system persists `my-job` to `~/.esdiag/jobs.yml`
- **AND** the command continues using the unchanged process execution arguments

#### Scenario: Incompatible invocation rejects save-job

- **WHEN** the user runs `esdiag collect --save-job my-job [ARGS]` or `esdiag process --save-job my-job [ARGS]` with an invocation that cannot become a valid saved job
- **THEN** the system exits with a non-zero code
- **AND** the command reports that the invocation is not compatible with saved-job persistence

#### Scenario: Save-job overwrites an existing job name

- **WHEN** the user runs a compatible `--save-job <name>` invocation and `<name>` already exists in `jobs.yml`
- **THEN** the system replaces the existing saved job definition
- **AND** the command continues execution with the unchanged command arguments

### Requirement: Shared Executable Job Model

The system SHALL model executable diagnostic work as a `Job` independent of whether the job is persisted. `SavedJobs` SHALL be a YAML map from job name to `Job`, and "saved" SHALL only describe persistence to `jobs.yml`.

#### Scenario: Job contains only executable states

- **WHEN** a job is constructed for collection, upload, or processing
- **THEN** the job action is represented as an explicit typed variant
- **AND** inactive builder fields and string sentinels are not persisted

#### Scenario: Bundle retention is separate from final output

- **WHEN** a job retains an intermediate diagnostic bundle in addition to producing its final action output
- **THEN** the optional `save_dir` records where that intermediate bundle is kept
- **AND** `save_dir` is not used as the required final output destination
- **AND** collect actions require `output_dir`
- **AND** process actions use `output_dir` only when the process output target is a directory

#### Scenario: Conversion rejects incomplete job signals

- **WHEN** CLI or UI signal input lacks a required collect host, action, or output
- **THEN** conversion rejects the input before persistence or execution

#### Scenario: Saved job loads into existing UI signal state

- **WHEN** a persisted `Job` is loaded by the Jobs page
- **THEN** the system projects it into the existing job signals for display and editing
- **AND** the persisted YAML remains the typed `Job` shape
