## MODIFIED Requirements

### Requirement: Dynamic Source Endpoint Resolution
The system SHALL resolve API endpoint queries dynamically using the target product and its corresponding `assets/<product>/sources.yml` mapping file. The system MUST support semver rules whose values resolve either directly to a URL string or to a structured source definition that includes `url` plus optional collection metadata such as pagination or space awareness. The stored version ranges MUST already be in native Rust `semver` form (normalized during reconciliation, see `source-reconciliation`), and the system SHALL resolve them with the stock `semver::VersionReq` matcher — it MUST NOT carry a runtime parser for the upstream Java/NPM semver dialect.

#### Scenario: Host version matches a string-based semver rule
- **GIVEN** a product source configuration with version rules for a data source
- **WHEN** the target host's version matches exactly one string-valued semver rule
- **THEN** the system returns the API query string corresponding to that semver rule

#### Scenario: Host version matches a structured semver rule
- **GIVEN** a Kibana `sources.yml` configuration whose matching version rule contains a structured object with `url`, `spaceaware`, and `paginate` fields
- **WHEN** the target host's version matches that rule
- **THEN** the system returns the configured URL
- **AND** the system preserves the associated collection metadata for later execution planning

#### Scenario: Host version matches no semver rule
- **GIVEN** a product source configuration with version rules for a data source
- **WHEN** the target host's version matches none of the defined semver rules
- **THEN** the system returns an error indicating the API is unsupported on this version

#### Scenario: Version ranges resolve with stock semver
- **GIVEN** a `sources.yml` whose version ranges are stored in native Rust `semver` form
- **WHEN** the runtime resolves a source for a target version
- **THEN** it uses the stock `semver::VersionReq` matcher directly
- **AND** it does not invoke any custom upstream-dialect compatibility parser

## ADDED Requirements

### Requirement: Collection Definition Carries Execution Metadata
The collection definition (`assets/<product>/sources.yml`) SHALL be the single source of truth for each data source's execution metadata, not just its collection path. Each source entry SHALL carry roughly the field set `{ key, versions, extension, subdir, retry, source_weight, processing_weight, streamable, processable, required, dependencies, collect_dependencies, tags }`. The system MUST derive the collect list, the diagnostic-type sets, and the process dispatch from this definition rather than from parallel hand-maintained lists in code.

#### Scenario: Execution metadata is read from the registry
- **GIVEN** a source entry defines `retry`, `source_weight`, `processing_weight`, `streamable`, `processable`, `required`, `dependencies`, `collect_dependencies`, and `tags`
- **WHEN** the collector or processor needs any of those values for that source
- **THEN** it reads them from the registry entry
- **AND** no equivalent value is hardcoded in `api.rs`, `ProcessingOptionDef`, or the `es_base_apis` lists

#### Scenario: Processable source can be omitted from a diagnostic type by tag
- **GIVEN** a source is marked `processable: true`
- **AND** it has `tags: support` but not `tags: standard`
- **WHEN** the system builds a `standard` collection plan
- **THEN** the source is not included
- **AND** it remains valid for explicit include and for processing existing bundles

#### Scenario: Metadata is overridable without recompiling
- **GIVEN** a user supplies `--sources` pointing at a modified `sources.yml`
- **WHEN** the system loads the collection definition
- **THEN** the overridden execution metadata (e.g. adjusted `source_weight`) takes effect for that run without a rebuild

### Requirement: Source Role — Collect-Only vs Processable
Each data source SHALL have a **role**. A *collect-only* source (e.g. an Elasticsearch `_cat` text API) is saved into the bundle for human reading and has no `DataSource`/`DocumentExporter` implementation; a *processable* source is additionally transformed and is marked `processable: true` with a typed implementation. A registry entry with no processor SHALL be treated as a valid collect-only source, never as a wiring gap. A same-stem `_cat` text API and its JSON sibling are two roles of one concept, not a namespace conflict.

#### Scenario: Collect-only source has no processor
- **GIVEN** a `_cat`/`.txt` source entry exists in `sources.yml` with no typed implementation registered
- **WHEN** the system builds the collect and process plans
- **THEN** the source is collected and saved into the bundle
- **AND** it is not scheduled for processing and its absence of a processor is not reported as an error

#### Scenario: Processable source requires a registered implementation
- **GIVEN** a source entry is marked processable
- **WHEN** the system validates the registry at startup
- **THEN** it MUST resolve exactly one registered typed implementation for that source

#### Scenario: User-facing processing options are registry-defined
- **GIVEN** a source entry has a `required` marker
- **WHEN** the system builds processing options for the CLI or web UI
- **THEN** the source appears as a user-facing option
- **AND** `required: true` makes that option non-deselectable

### Requirement: Processable Source Key Alignment
For every *processable* source, its process-selection/dispatch key MUST equal its registry key and its `DataSource::name()`. The system SHALL treat this as an invariant enforced at startup: each processable key resolves to exactly one registry entry and one registered implementation. Existing drift between dispatch keys and registry keys (e.g. `pending_tasks` versus `cluster_pending_tasks`) MUST be reconciled to a single canonical key. Legacy input names SHALL remain accepted as aliases for compatibility, but generated catalogs and newly written manifests SHALL use canonical keys.

#### Scenario: Aligned key attaches all metadata to one source
- **GIVEN** the canonical key `cluster_pending_tasks` is used by the registry entry, the dispatch table, and `DataSource::name()`
- **WHEN** the system resolves that source's weight, `streamable` flag, and type membership
- **THEN** all attach to the single canonical key with no duplicate or drifting entry

#### Scenario: Unaligned processable key fails fast
- **GIVEN** a processable source whose dispatch key does not match any registry key
- **WHEN** the system validates the registry at startup
- **THEN** it MUST fail with an error rather than silently collecting-but-not-processing the source

#### Scenario: Legacy source names canonicalize
- **GIVEN** a user or saved job selects a legacy source name such as `pending_tasks`
- **WHEN** the system resolves requested source keys
- **THEN** it canonicalizes the request to the registry key `cluster_pending_tasks`
- **AND** it does not preserve the legacy key in the execution plan

### Requirement: Separate Collect and Processing Dependencies
The registry SHALL distinguish process-time prerequisites (`dependencies`) from collect-time prerequisites (`collect_dependencies`). The system MUST use `dependencies` when resolving processing selections and `collect_dependencies` when resolving collect plans, because a source can require different prerequisites in each stage.

#### Scenario: Processing dependencies do not drive collect planning
- **GIVEN** a source defines a processing dependency in `dependencies`
- **WHEN** the collector resolves APIs to fetch
- **THEN** it does not use that processing dependency unless the same key is also present in `collect_dependencies`

#### Scenario: Collect dependencies are collected with their dependent source
- **GIVEN** a source defines a collect prerequisite in `collect_dependencies`
- **WHEN** the user includes that source for collection
- **THEN** the collect plan also includes the prerequisite source

### Requirement: Two-Axis Source Weight
A data source's scheduling cost SHALL be expressed as two orthogonal graded per-source axes in the collection definition: `source_weight` (load imposed on the system the source is pulled from) and `processing_weight` (ESDiag CPU/time to transform it), replacing the legacy binary `ApiWeight { Heavy, Light }`. `source_weight` SHALL govern only collect concurrency and `processing_weight` SHALL govern only processing concurrency. The mapping from a weight to a concurrency limit is deployment-tunable policy and MUST NOT be a hardcoded constant.

#### Scenario: Collect concurrency uses source weight only
- **GIVEN** a source with a high `source_weight` and a low `processing_weight`
- **WHEN** the collector schedules it
- **THEN** it constrains collect concurrency according to `source_weight`
- **AND** `processing_weight` does not influence collect scheduling

#### Scenario: Asymmetric-cost source is scheduled independently per stage
- **GIVEN** a source that is cheap to fetch but expensive to transform (low `source_weight`, high `processing_weight`)
- **WHEN** the source is collected and later processed
- **THEN** collect scheduling treats it as light and processing scheduling treats it as heavy

#### Scenario: Legacy weight maps onto the source-weight scale
- **GIVEN** a source previously marked `Heavy` or `Light`
- **WHEN** its definition is migrated
- **THEN** the legacy value maps onto the graded `source_weight` scale

#### Scenario: Weight policy is deployment-tunable
- **GIVEN** deployment environment variables configure collect or processing concurrency thresholds
- **WHEN** collection or processing schedules sources by weight
- **THEN** the scheduler uses the configured thresholds
- **AND** unset variables fall back to defaults that preserve the legacy concurrency shape

### Requirement: Explicit Streamable Flag
Whether a source is streamed during processing SHALL be an explicit `streamable` flag in the collection definition, not implied by which dispatch function is called. The system SHALL route a source through the streaming processing path if and only if its `streamable` flag is set.

#### Scenario: Streaming is driven by the flag
- **GIVEN** a source entry with `streamable: true`
- **WHEN** the processor dispatches it
- **THEN** it uses the streaming processing path
- **AND** a source with `streamable` unset or false uses the buffered path
