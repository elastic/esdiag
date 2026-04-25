# saved-jobs

## Purpose

Defines persistence, retrieval, and execution of named diagnostic job configurations. A saved job is a persisted `Job` that captures executable diagnostic work plus metadata so it can be re-run later from the web UI or CLI without reconfiguration.

## Requirements

### Requirement: Job Configuration Persistence
The system SHALL persist named job configurations to `~/.esdiag/jobs.yml` as a YAML map from job name to `Job`. A `Job` SHALL contain collection input, an explicit executable action, and optional `Identifiers` metadata. No session-specific or credential-bearing state SHALL be included in the persisted payload. Saved jobs therefore depend on persisted known-host definitions from `hosts.yml` rather than embedding API keys, passwords, or other secrets inside `jobs.yml`.

#### Scenario: Save new job
- **WHEN** the user provides a non-empty name and clicks Save on the `/jobs` page
- **THEN** the current job signals and metadata are written to `~/.esdiag/jobs.yml` under that name
- **AND** the saved job appears in the left-panel job list without a page reload

#### Scenario: Overwrite existing job
- **WHEN** the user saves with a name that already exists in `jobs.yml`
- **THEN** the existing entry is replaced with the current configuration

#### Scenario: Reject empty name
- **WHEN** the user attempts to save with an empty or whitespace-only name
- **THEN** the system rejects the request with a validation error and makes no change to `jobs.yml`

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

### Requirement: Valid Collect Sources for Saved Jobs
Only known-host collection SHALL be valid for saved jobs. Direct API key collection, direct uploads, and service link downloads either depend on non-persistent credentials or reference one-time paths/URIs and therefore are not repeatable. The Save button SHALL be disabled when the job signals are configured for any collect source other than known host.

#### Scenario: Save disabled for upload job signals
- **WHEN** the job signal collect source is set to direct file upload
- **THEN** the Save button is disabled and cannot be clicked

#### Scenario: Save disabled for service link job signals
- **WHEN** the job signal collect source is set to a service link
- **THEN** the Save button is disabled and cannot be clicked

#### Scenario: Save enabled for known-host job signals
- **WHEN** the job signal collect source is set to a known host
- **THEN** the Save button is enabled

#### Scenario: Save disabled for API key job signals
- **WHEN** the job signal collect source is set to an API key
- **THEN** the Save button is disabled and cannot be clicked

### Requirement: Saved Jobs Use Persisted Known Hosts
Saved jobs SHALL be created and executed only for known hosts that exist in `hosts.yml`. If a referenced host uses a keystore `secret`, that credential SHALL still be resolved at runtime. Known hosts that use no authentication SHALL also remain valid saved-job collection sources.

#### Scenario: Save allowed for host without secret reference
- **GIVEN** the selected known host uses no authentication
- **WHEN** the user attempts to save the job
- **THEN** the save succeeds

#### Scenario: Run allowed for host without secret reference
- **GIVEN** a saved job references a known host that exists in `hosts.yml` and uses no authentication
- **WHEN** `esdiag job run <name>` is executed for that job
- **THEN** the system runs the saved job using that host configuration

### Requirement: Default Job Name
The system SHALL derive a default job name from the current job signals using the pattern `{host}-{action}-{destination}`, pre-populating the name field so the user can accept or override it before saving.

- **host**: the known host name from the collect stage
- **action**: `collect` when only collecting; `process` when processing
- **destination**:
  - collect + save to local file -> `save`
  - collect + upload to upload service -> `upload`
  - process + send to remote host -> the target host name
  - process + write to local file -> `file`
  - process + write to local directory -> `directory`

#### Scenario: Default name for collect-save
- **WHEN** the job signals are configured to collect from host `prod` and save locally
- **THEN** the name field is pre-populated with `prod-collect-save`

#### Scenario: Default name for collect-upload
- **WHEN** the job signals are configured to collect from host `es_poc` and upload to the upload service
- **THEN** the name field is pre-populated with `es_poc-collect-upload`

#### Scenario: Default name for process to remote host
- **WHEN** the job signals are configured to process and send to remote host `monitoring`
- **THEN** the name field is pre-populated with `prod-process-monitoring`

#### Scenario: Default name for process to disk
- **WHEN** the job signals are configured to process and write to a local directory
- **THEN** the name field is pre-populated with `prod-process-directory`

#### Scenario: User overrides default name
- **WHEN** the name field is pre-populated with a default and the user edits it before saving
- **THEN** the job is saved under the user-provided name

### Requirement: Saved Job Listing
The system SHALL expose a list of saved job names to the Job Builder web UI only when `ServerPolicy` allows the `job-builder` web feature. When exposed, the list SHALL reflect the current contents of `jobs.yml` and update after every save or delete operation.

#### Scenario: Jobs listed on page load
- **GIVEN** the web server is running in `user` mode
- **AND** the `job-builder` web feature is enabled
- **WHEN** the user navigates to the `/jobs` page
- **THEN** the left panel displays all saved job names from `jobs.yml`

#### Scenario: Empty state
- **GIVEN** the web server is running in `user` mode
- **AND** the `job-builder` web feature is enabled
- **WHEN** `jobs.yml` does not exist or contains no entries
- **THEN** the left panel displays an empty state message (e.g., "No saved jobs")

#### Scenario: Web listing unavailable when Job Builder disabled
- **GIVEN** the web server is running in `user` mode
- **AND** the `job-builder` web feature is disabled
- **WHEN** the user requests `/jobs/saved`
- **THEN** the saved-job web listing endpoint is not mounted

### Requirement: Load Saved Job into UI
The system SHALL restore a saved job's full job signals into the Job Builder page signal state when `ServerPolicy` allows the `job-builder` web feature and the user selects it from the left panel.

#### Scenario: Select saved job restores signal state
- **GIVEN** the web server is running in `user` mode
- **AND** the `job-builder` web feature is enabled
- **WHEN** the user selects a job name from the left panel
- **THEN** the `/jobs` page is rendered with the saved job's signal state and identifiers pre-populated in the initial signals
- **AND** the user can immediately run or further modify the loaded configuration

#### Scenario: Load unknown job name via URL
- **GIVEN** the web server is running in `user` mode
- **AND** the `job-builder` web feature is enabled
- **WHEN** the user navigates to `/jobs/saved/:name` and the named job does not exist in `jobs.yml`
- **THEN** the `/jobs` page is rendered with a "Job <name> not found" message

#### Scenario: Load route unavailable when Job Builder disabled
- **GIVEN** the web server is running in `user` mode
- **AND** the `job-builder` web feature is disabled
- **WHEN** the user requests `/jobs/saved/:name`
- **THEN** the saved-job web load endpoint is not mounted

### Requirement: Delete Saved Job
The system SHALL allow the user to delete a saved job by name from the Job Builder page only when `ServerPolicy` allows the `job-builder` web feature. Deletion SHALL remove the entry from `jobs.yml` and refresh the left-panel list.

#### Scenario: Delete job removes entry
- **GIVEN** the web server is running in `user` mode
- **AND** the `job-builder` web feature is enabled
- **WHEN** the user triggers delete for a named job
- **THEN** the entry is removed from `jobs.yml` and disappears from the left panel

#### Scenario: Delete route unavailable when Job Builder disabled
- **GIVEN** the web server is running in `user` mode
- **AND** the `job-builder` web feature is disabled
- **WHEN** the user sends a delete request for `/jobs/saved/:name`
- **THEN** the saved-job web delete endpoint is not mounted

### Requirement: CLI Job Listing
The system SHALL provide `esdiag job list` as a CLI subcommand that prints a text table of all saved jobs from `~/.esdiag/jobs.yml`. The table SHALL include the columns **Name**, **Collection target**, **Processing**, and **Send target**. CLI job listing SHALL NOT depend on `ServerPolicy`, runtime mode, or `ESDIAG_WEB_FEATURES`.

#### Scenario: List saved jobs
- **WHEN** the user runs `esdiag job list` and `jobs.yml` contains entries
- **THEN** the system prints a text table describing each saved job and exits with code 0

#### Scenario: List with no saved jobs
- **WHEN** the user runs `esdiag job list` and `jobs.yml` does not exist or is empty
- **THEN** the system prints nothing (or an informative message) and exits with code 0

#### Scenario: Web feature flags do not affect CLI listing
- **GIVEN** `ESDIAG_WEB_FEATURES` is set to an empty string
- **WHEN** the user runs `esdiag job list`
- **THEN** CLI listing behavior is unchanged

### Requirement: CLI Job Execution
The system SHALL provide `esdiag job run <name>` as a CLI subcommand that loads the named job from `~/.esdiag/jobs.yml` and executes it using the existing CLI collect/process pipeline. CLI job execution SHALL NOT depend on `ServerPolicy`, runtime mode, or `ESDIAG_WEB_FEATURES`.

#### Scenario: Run saved job by name
- **WHEN** the user runs `esdiag job run my-job`
- **THEN** the system loads `my-job` from `~/.esdiag/jobs.yml` and executes the full job
- **AND** exits with code 0 on success

#### Scenario: Unknown job name
- **WHEN** the user runs `esdiag job run unknown-name` and that name is not in `jobs.yml`
- **THEN** the system exits with a non-zero code and a clear error message naming the missing job

#### Scenario: Missing jobs file
- **WHEN** `~/.esdiag/jobs.yml` does not exist
- **THEN** `esdiag job run` exits with a non-zero code and an informative error message

#### Scenario: Stale host reference
- **GIVEN** a saved job references a known host that no longer exists in `hosts.yml`
- **WHEN** `esdiag job run` is executed for that job
- **THEN** the system exits with a non-zero code and an error identifying the missing host

#### Scenario: Web feature flags do not affect CLI execution
- **GIVEN** `ESDIAG_WEB_FEATURES` is set to an empty string
- **WHEN** the user runs `esdiag job run my-job`
- **THEN** CLI execution behavior is unchanged

### Requirement: CLI Job Deletion
The system SHALL provide `esdiag job delete <name>` as a CLI subcommand that removes the named job from `~/.esdiag/jobs.yml`. CLI job deletion SHALL NOT depend on `ServerPolicy`, runtime mode, or `ESDIAG_WEB_FEATURES`.

#### Scenario: Delete existing job
- **WHEN** the user runs `esdiag job delete my-job` and `my-job` exists in `jobs.yml`
- **THEN** the entry is removed from `jobs.yml` and the command exits with code 0

#### Scenario: Delete unknown job name
- **WHEN** the user runs `esdiag job delete unknown-name` and that name is not in `jobs.yml`
- **THEN** the system exits with a non-zero code and a clear error message naming the missing job

#### Scenario: Web feature flags do not affect CLI deletion
- **GIVEN** `ESDIAG_WEB_FEATURES` is set to an empty string
- **WHEN** the user runs `esdiag job delete my-job`
- **THEN** CLI deletion behavior is unchanged

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
