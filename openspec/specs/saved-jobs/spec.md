# saved-jobs

## Purpose

Defines persistence, retrieval, and execution of named diagnostic job configurations. A saved job captures a complete workflow (collect -> process -> send) plus metadata so it can be re-run later from the web UI or CLI without reconfiguration.

## Requirements

### Requirement: Job Configuration Persistence
The system SHALL persist named job configurations to `~/.esdiag/jobs.yml` as a YAML map from job name to `SavedJob`. A `SavedJob` SHALL contain the workflow stages (collect, process, send) and optional `Identifiers` metadata. No session-specific or credential-bearing state SHALL be included in the persisted payload. Saved jobs therefore depend on persisted known-host definitions from `hosts.yml` rather than embedding API keys, passwords, or other secrets inside `jobs.yml`.

#### Scenario: Save new job
- **WHEN** the user provides a non-empty name and clicks Save on the `/jobs` page
- **THEN** the current workflow configuration and metadata are written to `~/.esdiag/jobs.yml` under that name
- **AND** the saved job appears in the left-panel job list without a page reload

#### Scenario: Overwrite existing job
- **WHEN** the user saves with a name that already exists in `jobs.yml`
- **THEN** the existing entry is replaced with the current configuration

#### Scenario: Reject empty name
- **WHEN** the user attempts to save with an empty or whitespace-only name
- **THEN** the system rejects the request with a validation error and makes no change to `jobs.yml`

### Requirement: Valid Collect Sources for Saved Jobs
Only known-host collection SHALL be valid for saved jobs. Direct API key collection, direct uploads, and service link downloads either depend on non-persistent credentials or reference one-time paths/URIs and therefore are not repeatable. The Save button SHALL be disabled when the workflow is configured for any collect source other than known host.

#### Scenario: Save disabled for upload workflow
- **WHEN** the workflow collect source is set to direct file upload
- **THEN** the Save button is disabled and cannot be clicked

#### Scenario: Save disabled for service link workflow
- **WHEN** the workflow collect source is set to a service link
- **THEN** the Save button is disabled and cannot be clicked

#### Scenario: Save enabled for known host workflow
- **WHEN** the workflow collect source is set to a known host
- **THEN** the Save button is enabled

#### Scenario: Save disabled for API key workflow
- **WHEN** the workflow collect source is set to an API key
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
The system SHALL derive a default job name from the current workflow configuration using the pattern `{host}-{action}-{destination}`, pre-populating the name field so the user can accept or override it before saving.

- **host**: the known host name from the collect stage
- **action**: `collect` when only collecting; `process` when processing
- **destination**:
  - collect + save to local file -> `save`
  - collect + upload to upload service -> `upload`
  - process + send to remote host -> the target host name
  - process + write to local file -> `file`
  - process + write to local directory -> `directory`

#### Scenario: Default name for collect-save
- **WHEN** the workflow is configured to collect from host `prod` and save locally
- **THEN** the name field is pre-populated with `prod-collect-save`

#### Scenario: Default name for collect-upload
- **WHEN** the workflow is configured to collect from host `es_poc` and upload to the upload service
- **THEN** the name field is pre-populated with `es_poc-collect-upload`

#### Scenario: Default name for process to remote host
- **WHEN** the workflow is configured to process and send to remote host `monitoring`
- **THEN** the name field is pre-populated with `prod-process-monitoring`

#### Scenario: Default name for process to disk
- **WHEN** the workflow is configured to process and write to a local directory
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
The system SHALL restore a saved job's full workflow configuration into the Job Builder page signal state when `ServerPolicy` allows the `job-builder` web feature and the user selects it from the left panel.

#### Scenario: Select saved job restores workflow
- **GIVEN** the web server is running in `user` mode
- **AND** the `job-builder` web feature is enabled
- **WHEN** the user selects a job name from the left panel
- **THEN** the `/jobs` page is rendered with the saved job's workflow and identifiers pre-populated in the initial signals
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
- **THEN** the system loads `my-job` from `~/.esdiag/jobs.yml` and executes the full workflow
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
