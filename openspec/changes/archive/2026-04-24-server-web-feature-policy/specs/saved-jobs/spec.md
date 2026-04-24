## MODIFIED Requirements

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
