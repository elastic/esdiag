## ADDED Requirements

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
`action` has exactly one target, with no unmapped fallthrough. This migration applies
only to `jobs.yml` (a file ESDiag owns and writes) and MUST NOT be applied to received
artifacts such as bundles or manifests.

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

#### Scenario: Migration is not applied to received artifacts
- **WHEN** a bundle or manifest is read
- **THEN** the saved-job migration MUST NOT run against it; received artifacts use additive read tolerance instead

## MODIFIED Requirements

### Requirement: Job Configuration Persistence
The system SHALL persist named job configurations to `~/.esdiag/jobs.yml` as a versioned
document containing a `schema_version` field and a YAML map from job name to `Job`. A
`Job` SHALL be phase-structured — a required `input` (`Collect`), an optional `save`
target, an optional `process` stage (carrying its own `export` target), an optional
`send` target, and optional `Identifiers` metadata. No session-specific or
credential-bearing state SHALL be included in the persisted payload. Saved jobs therefore
depend on persisted known-host definitions from `hosts.yml` rather than embedding API
keys, passwords, or other secrets inside `jobs.yml`.

#### Scenario: Save new job
- **WHEN** the user provides a non-empty name and clicks Save on the `/jobs` page
- **THEN** the current job signals and metadata are written to `~/.esdiag/jobs.yml` under that name as a phase-based `Job`
- **AND** the file records the current `schema_version`
- **AND** the saved job appears in the left-panel job list without a page reload

#### Scenario: Overwrite existing job
- **WHEN** the user saves with a name that already exists in `jobs.yml`
- **THEN** the existing entry is replaced with the current configuration

#### Scenario: Reject empty name
- **WHEN** the user attempts to save with an empty or whitespace-only name
- **THEN** the system rejects the request with a validation error and makes no change to `jobs.yml`

### Requirement: Shared Executable Job Model
The system SHALL model executable diagnostic work as a phase-based `Job` independent of
whether the job is persisted. A `Job` SHALL select stages within three phases — `input`
(`Collect`), an optional middle (`save` and/or `process`), and an optional output
(`send` and/or the `process` stage's `export`). `SavedJobs` SHALL be a YAML map from job
name to `Job`, and "saved" SHALL only describe persistence to `jobs.yml`.

#### Scenario: Job phases are explicit and typed
- **WHEN** a job is constructed for collection, sending, or processing
- **THEN** each active stage is represented as an explicit typed phase (`input`/`save`/`process`/`send`)
- **AND** inactive builder fields and string sentinels are not persisted

#### Scenario: Bundle retention is separate from final output
- **WHEN** a job retains an intermediate diagnostic bundle in addition to producing its final output
- **THEN** the optional `save` target records where that intermediate bundle is kept
- **AND** a `save` target is present only when `input` is `Collect`
- **AND** `process` carries its own `export` target for processed documents, distinct from `save`

#### Scenario: Conversion rejects incomplete job signals
- **WHEN** CLI or UI signal input lacks a required collect host, or specifies no `save`/`process`/`send`
- **THEN** conversion rejects the input before persistence or execution

#### Scenario: Saved job loads into existing UI signal state
- **WHEN** a persisted `Job` is loaded by the Jobs page
- **THEN** the system projects it into the existing job signals for display and editing
- **AND** the persisted YAML remains the typed phase-based `Job` shape
