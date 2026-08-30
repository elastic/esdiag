# saved-jobs

## Purpose

Defines persistence, retrieval, and execution of named diagnostic job configurations. A saved job is a persisted `Job` that captures executable diagnostic work plus metadata so it can be re-run later from the web UI or CLI without reconfiguration.
## Requirements
### Requirement: Versioned Saved-Job Schema
The system SHALL record a `schema_version` field in the persisted `jobs.yml` payload
identifying the on-disk saved-job schema. An **absent** `schema_version` SHALL mean
**v1** — the legacy `Job { collect, action }` shape. On load, the system SHALL dispatch
on `schema_version` deterministically: an absent version routes to legacy migration, and
the current version deserializes directly into the phase-based `Job`. The system SHALL
NOT infer the schema by inspecting the shape of individual entries.

#### Scenario: Absent version treated as v1
- **WHEN** `jobs.yml` is loaded and contains no `schema_version` field
- **THEN** the system MUST treat the file as schema v1 and route its entries through legacy migration

#### Scenario: Current version loaded directly
- **WHEN** `jobs.yml` is loaded and its `schema_version` matches the current schema
- **THEN** the system MUST deserialize each entry directly as a phase-based `Job` without migration or rewrite

#### Scenario: No shape sniffing
- **WHEN** the system loads `jobs.yml`
- **THEN** schema selection MUST be driven solely by `schema_version` and MUST NOT depend on which fields an individual entry happens to contain

### Requirement: Legacy Saved-Job Migration on First Read
When `jobs.yml` is schema v1 (absent `schema_version`), the system SHALL map every entry
to the phase-based `Job` of ADR-0004 via a closed, total `From<LegacyJob>`, and — if any
entry was legacy — SHALL rewrite the whole file in the new shape on first read using the
existing atomic-write plumbing (`write_yaml_atomic` / `replace_file_atomic` /
`secure_output_file`). Every legacy saved job is collect-first, so `input` MUST be
`Collect`. The legacy `action` MUST map as:

- `Collect { output_dir }` → `save: Some(output_dir)`
- `Upload { upload_id }` → `save: Some(dir)`, `send: Some(upload_id)`
- `Process { output, selection }` → `save: save_dir?`, `process: Some { selection, export: output }`

A migrated `Process` job that has no `save_dir` SHALL become **streaming** (no `Save`)
rather than the legacy always-staged behavior. The mapping SHALL be total: every legacy
`action` has exactly one target, with no unmapped fallthrough, and no v1 entry SHALL be
rejected on the basis of the current source registry — a legacy `selection` naming a
product or source that is no longer registered SHALL be carried through as authored and
left for execution to reject. This migration applies only to `jobs.yml` (a file ESDiag
owns and writes) and MUST NOT be applied to received artifacts such as bundles or
manifests.

#### Scenario: Legacy file is migrated and rewritten on first read
- **WHEN** a `jobs.yml` with no `schema_version` and one or more legacy entries is loaded
- **THEN** each entry MUST be mapped to the phase-based `Job` via the closed `From<LegacyJob>`
- **AND** the whole file MUST be rewritten in the new shape with `schema_version` set, using the atomic-write plumbing
- **AND** a subsequent load MUST deserialize directly with no further migration or rewrite

#### Scenario: Migrate legacy Collect action
- **WHEN** a legacy entry has `action: Collect { output_dir }`
- **THEN** the migrated `Job` MUST have `input: Collect`, `save: Some(output_dir)`, and no `process` or `send`

#### Scenario: Migrate legacy Upload action
- **WHEN** a legacy entry has `action: Upload { upload_id }`
- **THEN** the migrated `Job` MUST have `input: Collect`, `save: Some(dir)`, and `send: Some(upload_id)`

#### Scenario: Migrate legacy Process action with a save directory
- **WHEN** a legacy entry has `action: Process { output, selection }` and its `collect.save_dir` is set
- **THEN** the migrated `Job` MUST have `input: Collect`, `save: Some(save_dir)`, and `process: Some { selection, export: output }` (staged)

#### Scenario: Migrated Process without save directory is streaming
- **WHEN** a legacy entry has `action: Process { output, selection }` and no `collect.save_dir`
- **THEN** the migrated `Job` MUST have `input: Collect`, no `save`, and `process: Some { selection, export: output }` (streaming)

#### Scenario: Migrate legacy Upload action without a save directory
- **WHEN** a legacy entry has `action: Upload { upload_id }` and no `collect.save_dir`
- **THEN** the migrated `Job` MUST stage a temporary bundle so the `send` target has one to upload

#### Scenario: Legacy selection the current registry rejects still migrates
- **WHEN** a legacy entry's `selection` names a product or source key the current source registry does not know
- **THEN** that entry MUST migrate with its selection carried through as authored
- **AND** the remaining entries in the file MUST migrate and the file MUST still be rewritten
- **AND** the resulting error MUST be raised when that one job is executed, not when the file is loaded

#### Scenario: Migration is not applied to received artifacts
- **WHEN** a bundle or manifest is read
- **THEN** the saved-job migration MUST NOT run against it; received artifacts use additive read tolerance instead

### Requirement: Job Configuration Persistence
The system SHALL persist named job configurations to `~/.esdiag/jobs.yml` as a
versioned document containing a `schema_version` and a YAML map from job name to
`Job`. A persisted Job SHALL use the saved-job-compatible subset: a known-host
`Collect` input, optional `save`, optional `process` with a stable `export`
target, optional `send`, and optional `Identifiers`. Stable output references
MAY name persisted hosts or filesystem targets. Runtime binding keys,
session-specific resources, and credential-bearing state SHALL NOT be persisted.

#### Scenario: Save new job
- **WHEN** the user provides a non-empty name and saves a compatible JobDraft
- **THEN** the backend MUST compile it into a phase-based Job under that name
- **AND** the file MUST record the current schema_version
- **AND** the saved job MUST appear in the job list without a page reload

#### Scenario: Overwrite existing job
- **WHEN** the user saves with a name that already exists in jobs.yml
- **THEN** the existing entry MUST be replaced with the compatible compiled Job

#### Scenario: Reject empty name
- **WHEN** the user attempts to save with an empty or whitespace-only name
- **THEN** the system MUST reject the request and make no change to jobs.yml

#### Scenario: Reject runtime-bound job
- **GIVEN** an ephemeral Job uses a runtime-bound input or output
- **WHEN** a caller attempts to persist it
- **THEN** saved-job validation MUST reject it before writing jobs.yml
- **AND** no runtime binding, credential, or execution resource may enter the persisted payload

### Requirement: Shared Executable Job Model
The system SHALL model executable diagnostic work as a phase-based `Job`
independent of whether it is persisted. An ephemeral Job MAY use `Collect` or
`Load` and MAY refer to execution-only resources through opaque runtime binding
keys resolved by `ExecutionContext`. A saved Job SHALL use only the stable,
repeatable subset defined by Job Configuration Persistence. `JobDraft` SHALL be
the editable web representation and SHALL compile into a validated Job rather
than weakening the Job model.

#### Scenario: Job phases are explicit and typed
- **WHEN** a Job is constructed for collection, loading, processing, exporting, or sending
- **THEN** each active stage MUST be represented as an explicit typed phase
- **AND** execution-only resources MUST remain behind context binding keys

#### Scenario: Bundle retention is separate from final output
- **WHEN** a live-collection Job retains an intermediate bundle and produces another output
- **THEN** the optional Save target MUST record where that newly collected bundle is kept
- **AND** Process MUST carry its own Export target, distinct from Save and Send

#### Scenario: Conversion rejects incomplete draft
- **WHEN** a JobDraft lacks a required input or selected output target
- **THEN** compilation MUST reject it before persistence or execution
- **AND** the draft MAY remain available for continued editing

#### Scenario: Saved job loads into draft state
- **WHEN** a persisted Job is loaded by the Jobs page
- **THEN** the system MUST project it into JobDraft state for display and editing
- **AND** recompiling an unchanged draft MUST preserve every selected phase and stable target

### Requirement: Valid Collect Sources for Saved Jobs
Only known-host collection SHALL be valid for saved jobs. Direct API-key
collection, direct uploads, service-link downloads, nested child inputs, and
other runtime-bound sources depend on transient resources or non-repeatable
references. The Save Job action SHALL be disabled when JobDraft is configured
for any input other than known-host Collect.

#### Scenario: Save disabled for upload draft
- **WHEN** the JobDraft input is a direct file upload
- **THEN** the Save Job action MUST be disabled

#### Scenario: Save disabled for service-link draft
- **WHEN** the JobDraft input is a service link
- **THEN** the Save Job action MUST be disabled

#### Scenario: Save enabled for known-host draft
- **WHEN** the JobDraft input is known-host Collect and all selected targets are stable
- **THEN** the Save Job action MUST be enabled

#### Scenario: Save disabled for API-key draft
- **WHEN** the JobDraft input is an ad-hoc API-key host
- **THEN** the Save Job action MUST be disabled

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
  - collect + send to Elastic Upload Service -> `send`
  - process + send to remote host -> the target host name
  - process + write to local file -> `file`
  - process + write to local directory -> `directory`

#### Scenario: Default name for collect-save
- **WHEN** the job signals are configured to collect from host `prod` and save locally
- **THEN** the name field is pre-populated with `prod-collect-save`

#### Scenario: Default name for collect-send
- **WHEN** the job signals are configured to collect from host `es_poc` and send to Elastic Upload Service
- **THEN** the name field is pre-populated with `es_poc-collect-send`

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
