## MODIFIED Requirements

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
