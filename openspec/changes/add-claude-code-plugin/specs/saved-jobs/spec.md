## MODIFIED Requirements

### Requirement: CLI Job Execution
The system SHALL provide `esdiag job run <name>` as a CLI subcommand that loads the named phase-composed job from `~/.esdiag/jobs.yml` and executes it through the unified job executor. CLI job execution SHALL NOT depend on `ServerPolicy`, runtime mode, or `ESDIAG_WEB_FEATURES`.

On success, `esdiag job run` SHALL report every durable result produced by the phase-composed job. A saved job conceals which stages ran, so its terminal result MUST identify the retained archive created by `Save`, the diagnostic identifier and Kibana link created by `Process`, and the upload destination created by `Send` whenever each is present. Multiple facts MAY coexist in one result. A temporary serialization bundle or `Load` input path MUST NOT be reported as a retained archive created by the run, and a job without `Process` MUST NOT imply that a diagnostic was created.

#### Scenario: Run saved job by name
- **WHEN** the user runs `esdiag job run my-job`
- **THEN** the system loads `my-job` from `~/.esdiag/jobs.yml` and executes the full job
- **AND** exits with code 0 on success

#### Scenario: Processing stage reports its diagnostic identifier
- **GIVEN** a saved job with a `Process` stage exporting to an output target
- **WHEN** the user runs `esdiag job run my-job`
- **THEN** the terminal result reports the diagnostic identifier the run created
- **AND** includes the Kibana link when one is available

#### Scenario: Retained save reports its archive path
- **GIVEN** a saved job with `Collect` input and a retained `Save` stage but no `Process`
- **WHEN** the user runs `esdiag job run my-job`
- **THEN** the terminal result reports the path of the collected archive
- **AND** does not report a diagnostic identifier

#### Scenario: Send stage reports its destination
- **GIVEN** a saved job with a `Send` stage
- **WHEN** the user runs `esdiag job run my-job`
- **THEN** the terminal result reports the upload destination

#### Scenario: One run reports every durable result
- **GIVEN** a saved job with retained `Save`, `Process`, and `Send` stages
- **WHEN** the user runs `esdiag job run my-job`
- **THEN** one terminal result includes the archive path, diagnostic identifier and Kibana link, and upload destination

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
