## ADDED Requirements

### Requirement: Execution Resources Are Supplied by Context
The system SHALL keep a `Job` as the validated phase declaration and SHALL
supply execution-only resources through an `ExecutionContext`. The context
SHALL resolve stable and runtime-bound input/output references, provide
role-typed bundle and document adapters, carry execution identity and observer,
and apply retained-bundle policy. Credentials, resolved adapters, server state,
and owner-scoped event channels SHALL NOT be serialized into a `Job`. Saved-job
persistence SHALL reject runtime-only binding references.

#### Scenario: Ephemeral API-key input executes without persistence
- **GIVEN** a web request supplies an ad-hoc host and one-use API key
- **WHEN** the request compiles a runtime-bound Collect job
- **THEN** the execution context MUST resolve that binding to the in-memory host
- **AND** the API key MUST NOT be written to the Job, hosts file, saved jobs, logs, or events

#### Scenario: Saved job uses stable references
- **GIVEN** a saved Job contains a known-host Collect input and stable output references
- **WHEN** the default execution context runs it
- **THEN** the context MUST resolve those stable references from existing configuration

#### Scenario: Saved job rejects runtime binding
- **GIVEN** an ephemeral Job contains a runtime-only input or output binding
- **WHEN** a caller attempts to persist it as a saved job
- **THEN** persistence MUST reject the Job before writing the saved-jobs file

#### Scenario: Service-link Load materializes a bundle
- **GIVEN** a Job loads an existing diagnostic from an Elastic Upload Service link
- **AND** Process, raw-bundle Send, or retained-download policy requires a local bundle
- **WHEN** the context resolves the Load input
- **THEN** it MUST materialize the remote input as a bundle for the selected consumer
- **AND** this retention MUST NOT be represented as a Save stage because Save remains Collect-only

### Requirement: Executor Returns Structured Outcomes and Events
The one executor SHALL return an `ExecutionOutcome` containing the result of
each selected stage, the parent diagnostic report and derived diagnostic
outcome when processing runs, every child diagnostic outcome, retained bundle
information, and the upload result. It SHALL publish lifecycle and progress
events through the execution context observer. CLI, asynchronous web, and
synchronous API responses SHALL be projections of this one result/event
contract rather than alternate execution paths.

#### Scenario: Process result remains available to caller
- **WHEN** a Job completes Process and Export
- **THEN** its ExecutionOutcome MUST include the completed diagnostic report and derived outcome
- **AND** callers MUST be able to render document counts, recorded events, links, duration, and child outcomes without rerunning processing

#### Scenario: Observer receives execution identity
- **WHEN** the executor publishes queued, started, progress, or completed events
- **THEN** each event MUST carry the JobID and owner from the execution context
- **AND** a web caller MUST be able to preserve owner-scoped event delivery

### Requirement: Export and Send Complete Independently
After Phase-1 input has resolved and a bundle exists, the executor SHALL
attempt selected Process/Export and raw-bundle Send outputs independently. Failure of one
output SHALL NOT suppress an attempt of the other, and the ExecutionOutcome
SHALL preserve both stage results. Failure to resolve or collect the input
SHALL prevent dependent stages from starting. The aggregate terminal status
SHALL be non-success when any selected parent stage hard-fails, while
preserving successful stage results for reporting. Child outcomes and
report-recorded partial results SHALL follow their capability-specific status
rules.

#### Scenario: Processing fails but raw bundle sends
- **GIVEN** a staged Job selects Process/Export and raw-bundle Send
- **WHEN** Process fails after the bundle materializes
- **THEN** the executor MUST still attempt Send
- **AND** the outcome MUST report the Process failure and Send result separately

#### Scenario: Export fails after processing completes
- **GIVEN** a staged Job selects Process/Export and raw-bundle Send
- **WHEN** Process produces a diagnostic report but Export hard-fails
- **THEN** the outcome MUST preserve the report and its DiagnosticOutcome derived from any recorded exporter events
- **AND** it MUST report the hard Export stage failure separately and still attempt Send

#### Scenario: Export records partial document results
- **WHEN** Export completes with rejected documents or transport failures recorded in the diagnostic report
- **THEN** the report's DiagnosticOutcome MUST derive from those events
- **AND** the outcome MUST preserve that verdict without treating it as an unrelated Send failure

#### Scenario: Export succeeds but Send fails
- **GIVEN** a staged Job selects Process/Export and raw-bundle Send
- **WHEN** Export succeeds and Send fails
- **THEN** the outcome MUST retain the successful diagnostic report and Export result
- **AND** it MUST also report the Send failure and a non-success aggregate status

#### Scenario: Input failure prevents outputs
- **WHEN** Collect or Load fails before producing resolved input
- **THEN** Process, Export, and Send MUST NOT start
- **AND** the input failure MUST be the terminal execution result

### Requirement: Every Execution Surface Uses the One Executor
Every production execution surface SHALL construct a `Job` and use the one
executor: CLI `collect`, `process`, and standalone `upload`; the asynchronous
web runner; synchronous `/api/api_key` and `/api/service_link` processing;
saved-job execution; and included-diagnostic processing. No production surface
SHALL construct `Collector` or `Processor` as
an independent operation after convergence.

#### Scenario: Synchronous API returns executor results
- **WHEN** a synchronous API request processes a parent diagnostic with included diagnostics
- **THEN** it MUST execute through the one executor
- **AND** its response array MUST project the parent and child outcomes returned by that execution

#### Scenario: CLI behavior is preserved through convergence
- **WHEN** CLI collect or process runs through the executor
- **THEN** existing input forms, source overrides, save-job behavior, output fallback, summaries, child links, and exit behavior MUST remain available

#### Scenario: Standalone upload uses the executor
- **WHEN** the user runs `esdiag upload <file> <upload_id> --api-url <url>`
- **THEN** the CLI MUST execute a `Load(File) + Send` Job through the one executor
- **AND** the execution context's sender MUST preserve the custom upload API URL

## MODIFIED Requirements

### Requirement: Collect Stage Product Scope
The `Collect` stage SHALL acquire live diagnostics only for the
**API-collectable** products — Elasticsearch, Kibana, and Logstash. The system
SHALL NOT API-collect Elastic Agent or any platform (ECE, ECK,
KubernetesPlatform); those diagnostics are product-provided and enter the
pipeline exclusively via `Load`. When a `Collect` request targets a product
outside the API-collectable set, the system SHALL refuse it as **out-of-scope by
design** — distinct from a not-yet-implemented error — and SHALL direct the
caller to acquire that diagnostic via `Load` (CLI `process` input or Web UI
`Upload`).

#### Scenario: Collect an API-collectable product
- **WHEN** a user runs `esdiag collect` against an Elasticsearch, Kibana, or Logstash host
- **THEN** the system constructs a `Collect` receiver and pulls the resolved live APIs

#### Scenario: Collect refuses Elastic Agent
- **WHEN** a user requests a `Collect` against an Elastic Agent target
- **THEN** the system MUST refuse the request as out-of-scope by design
- **AND** the message MUST state that Elastic Agent provides its own diagnostic bundle to be acquired via `Load`, not API-collected

#### Scenario: Collect refuses a platform target
- **WHEN** a user requests a `Collect` against an ECE, ECK, or KubernetesPlatform target
- **THEN** the system MUST refuse the request as out-of-scope by design
- **AND** the message MUST direct the caller to `Load` the platform-generated bundle via CLI `process` input or Web UI `Upload`

#### Scenario: By-design refusal is not a not-yet-implemented error
- **WHEN** a `Collect` request is refused because its target is not an API-collectable product
- **THEN** the refusal MUST be reported as a deliberate scope boundary
- **AND** it MUST NOT be reported as unimplemented or work-in-progress collection

## REMOVED Requirements

### Requirement: One-Job and Two-Job Workflow Modes
**Reason**: The one-/two-job boundary was an artifact of the always-staged
legacy web path. Under the unified model one `Job` derives streaming versus
staged execution from its selected phases; saving a newly collected bundle
does not create a second Job. This requirement is removed only after the web
runner converges, because the legacy saved-bundle path uses two jobs until then.

**Migration**: UI `Collect -> Process -> Send` where Send denotes only the
processed-document Export target becomes one streaming
`Collect + Process/Export` Job when Save is absent. Raw-bundle Send from a live
Collect requires Save (temporary or retained) and is one staged
`Collect + Save + Send` Job, optionally also selecting Process/Export. A loaded
existing bundle may select Process/Export, Send, or both without a Save stage.
The previous saved web handoff becomes one staged Job whose executor uses the
materialized bundle as the serialization barrier.
