# Collection Execution

## Purpose

Defines how a resolved collection plan is executed: which products the `Collect` stage
may acquire live, how the plan is deduplicated and scheduled, and how collection hands
off to downstream stages.
## Requirements
### Requirement: API Deduplication
The system SHALL ensure that no API identifier appears more than once in the final resolved list of APIs to collect.

#### Scenario: Explicit inclusion overlaps with dependency
- **GIVEN** a diagnostic type that already includes `nodes`
- **WHEN** the user runs `esdiag collect --include nodes_stats,nodes`
- **THEN** the system resolves the final list of APIs
- **AND** the `nodes` API is only executed once during the collection phase

### Requirement: Safety-Aware Execution Concurrency
The system SHALL classify registered APIs as either `Heavy` or `Light`. `Heavy` APIs MUST be executed strictly sequentially to protect the target cluster from excessive load. `Light` APIs MAY be executed concurrently (with a bounded concurrency limit) to improve collection speed.

#### Scenario: Executing a mix of APIs
- **GIVEN** `nodes_stats` is classified as `Heavy` and `cluster_health` is classified as `Light`
- **WHEN** the system begins the execution phase of the collection
- **THEN** the `nodes_stats` API is fetched sequentially without other APIs executing concurrently
- **AND** the `cluster_health` API can be fetched concurrently alongside other `Light` APIs (e.g., `licenses`)

### Requirement: Graceful API Retries
The system SHALL implement a graceful retry mechanism for individual API fetch failures during collection. If a fetch fails due to a transient error, the system MUST retry the fetch using an exponential backoff timer for up to 5 minutes and log a warning.

#### Scenario: API fetch encounters a timeout
- **GIVEN** the collection execution loop is attempting to fetch `indices_stats`
- **WHEN** the HTTP request to the cluster times out
- **THEN** the system logs a warning detailing the failure
- **AND** the system retries the `indices_stats` request using exponential backoff
- **AND** if the retries continue to fail for 5 minutes, the system continues to the next API in the queue rather than aborting the entire collection run

### Requirement: Exhaustive API Matching
The system MUST implement exhaustive pattern matching when mapping the generic API enum to the concrete fetch/save execution logic to prevent unhandled APIs at compile time.

#### Scenario: Developer adds a new API enum variant
- **GIVEN** a developer adds a new variant `IndicesRecovery` to the `ElasticsearchApi` enum
- **WHEN** they attempt to compile the `esdiag` CLI
- **THEN** the Rust compiler issues an error because the new variant is not handled in the exhaustive `match` statement within the collection execution loop

### Requirement: Role-Constrained Execution Targets
The collection execution workflow SHALL resolve host targets by role before executing each workflow phase. The collect phase SHALL use only hosts with the `collect` role, the send phase SHALL use only hosts with the `send` role, and the view phase SHALL use only hosts with the `view` role.

#### Scenario: Resolve targets for multi-phase workflow
- **GIVEN** host configuration includes hosts with `collect`, `send`, and `view` roles
- **WHEN** the workflow resolves targets for collection and output handling
- **THEN** collection calls are executed only against `collect` hosts
- **AND** send/output calls are executed only against `send` hosts
- **AND** view target resolution includes only `view` hosts

### Requirement: Remote Collection Bundle Persistence
The workflow SHALL support optionally retaining a remotely collected diagnostic archive as a downloadable bundle before later workflow stages execute. If bundle saving is disabled, the workflow MAY continue with the in-memory or temporary workflow input without retaining a downloadable copy.

#### Scenario: Save retains a remotely collected bundle for download
- **GIVEN** the user starts a remote diagnostic collection and enables `Save Bundle`
- **WHEN** the collection completes successfully
- **THEN** the system retains the collected archive as a downloadable bundle
- **AND** subsequent processing or send steps consume that retained archive or its equivalent normalized workflow input

#### Scenario: Saved workflow bundle downloads outside the SSE stream
- **GIVEN** the workflow enables archive saving during remote collection
- **WHEN** the collected archive is ready for download
- **THEN** the browser fetches the bundle through a separate HTTP request or browser action
- **AND** the workflow status stream continues independently over SSE

### Requirement: Collect-Without-Process Workflow
The system SHALL support transmitting a diagnostic bundle without invoking processing when a
job selects a `Send` stage and no `Process` stage. The bundle MAY originate from `Collect` +
`Save` this run or from a `Load` input.

#### Scenario: Collect and save then send without processing
- **GIVEN** a job configured with `Collect`, `Save`, and `Send` and no `Process`
- **WHEN** the job runs
- **THEN** the system completes collection and materialises the bundle without creating processed diagnostic documents
- **AND** the `Send` stage transmits that saved bundle

#### Scenario: Load then send without processing
- **GIVEN** a job configured with `Load` input and a `Send` stage and no `Process`
- **WHEN** the job runs
- **THEN** the system transmits the loaded bundle without creating processed diagnostic documents

#### Scenario: Web forward workflow sends without processing
- **GIVEN** the user has configured a valid collect source
- **AND** the `Process` stage is configured for forwarding
- **WHEN** the workflow runs through collection and send
- **THEN** the system completes the collect stage without creating processed diagnostic documents
- **AND** the send stage receives the collected archive as its workflow input

### Requirement: Phase-Composed Job
The system SHALL model one diagnostic execution as a `Job` that selects stages within three
ordered phases: **Phase 1 (input, required)** is `Collect` xor `Load`; **Phase 2 (middle,
optional)** is `Save` and/or `Process`; **Phase 3 (output, optional)** is `Export` and/or
`Send`. `Export` SHALL live inside `Process` (`Process { selection, export }`) so that a
`Process` stage always has an export sink and an export can exist only with processing. The
`Job` constructor SHALL enforce the dependency invariants and reject any job that violates
them: `Save` requires `Collect` input; `Send` requires that a bundle exists (`Load` input or
`Save` set); and at least one of `Save`, `Process`, or `Send` MUST be selected.

#### Scenario: Save without collect is rejected
- **WHEN** a `Job` is constructed with `Load` input and a `Save` stage
- **THEN** construction MUST fail because `Save` requires `Collect` input

#### Scenario: Send without a bundle is rejected
- **WHEN** a `Job` is constructed with `Collect` input, a `Process` stage, no `Save`, and a `Send` stage
- **THEN** construction MUST fail because `Send` requires an existing bundle (`Load` input or `Save`)

#### Scenario: A job must do something
- **WHEN** a `Job` is constructed with a `Collect` input and no `Save`, `Process`, or `Send`
- **THEN** construction MUST fail because the job selects no Phase-2 or Phase-3 stage

#### Scenario: Export cannot exist without process
- **WHEN** a `Job` is expressed with an export target but no processing
- **THEN** the model MUST make that state unrepresentable because `Export` lives inside `Process`

### Requirement: Derived Execution Mode
The system SHALL derive a job's execution mode from its stage selection rather than storing
it. A job that selects both `Save` and `Process` SHALL execute in **staged** mode, where
collection completes and the bundle materialises before processing reads it (the bundle is a
serialization barrier). A job that selects `Collect` and `Process` without `Save` SHALL
execute in **streaming** mode, where receive, transform, and export overlap concurrently. A
single executor SHALL drive both modes.

#### Scenario: Save plus process is staged
- **WHEN** a job selects `Collect`, `Save`, and `Process`
- **THEN** the executor MUST complete collection and materialise the bundle before processing begins

#### Scenario: Collect plus process without save is streaming
- **WHEN** a job selects `Collect` and `Process` with no `Save`
- **THEN** the executor MUST overlap receiving, transforming, and exporting concurrently
- **AND** MUST NOT require an intermediate bundle to materialise first

#### Scenario: One executor drives both modes
- **WHEN** a `Job` is executed, whether its derived mode is staged or streaming
- **THEN** it MUST run through the same executor, which selects its strategy from the derived mode
- **AND** no second executor or mode-specific operation type MUST exist for either mode

### Requirement: Load Input Jobs
The system SHALL support jobs whose Phase-1 input is `Load` — reading an existing diagnostic
from a directory or bundle — in place of `Collect`. A `Load`-input job MAY select `Process`
and/or `Send` (a bundle already exists) but MUST NOT select `Save`.

#### Scenario: Load then process
- **WHEN** a job is configured with `Load` input over an existing bundle and a `Process` stage
- **THEN** the executor MUST read the loaded bundle as its input and produce processed documents
- **AND** MUST NOT perform any live collection

#### Scenario: Load then send
- **WHEN** a job is configured with `Load` input and a `Send` stage and no `Process`
- **THEN** the executor MUST transmit the loaded bundle without producing processed documents

### Requirement: Concurrent Export and Send
The system SHALL allow a single job to select both `Export` (processed documents) and `Send`
(the raw bundle) in Phase 3, executing both outputs in one run. `Export` and `Send` are
independent and SHALL NOT be mutually exclusive when their preconditions are met.

#### Scenario: Save, process, export, and send in one run
- **WHEN** a job selects `Collect`, `Save`, `Process` (with an export sink), and `Send`
- **THEN** the executor MUST index the processed documents to the export destination
- **AND** MUST also transmit the saved raw bundle via `Send` in the same run

### Requirement: Collect Command Optional Upload Handoff
The system SHALL allow `esdiag collect` to accept an optional `--upload` argument containing an Elastic Upload Service upload identifier or URL. When this argument is present, the collect command SHALL perform its normal collection behavior first and then begin an upload step for the archive it just produced. The existing `-u` shorthand SHALL remain reserved for the collect command's `--user` metadata option.

#### Scenario: Collect succeeds with upload handoff enabled
- **GIVEN** the user provides a valid collect host, a valid local output location, and a valid Elastic Upload Service upload identifier
- **WHEN** the user runs `esdiag collect <host> <output> --upload <upload_id>`
- **THEN** the system completes the collect step and writes a local diagnostic archive
- **AND** the system begins an upload step for that collected archive using the provided `upload_id`

#### Scenario: Collect without upload flag remains unchanged
- **GIVEN** the user provides a valid collect host and a valid local output location
- **WHEN** the user runs `esdiag collect <host> <output>` without `--upload`
- **THEN** the system completes the collect step and writes a local diagnostic archive
- **AND** the system does not invoke the Elastic Upload Service uploader

### Requirement: Collect Upload Handoff Uses Resolved Archive Path
The collect upload handoff SHALL use the actual archive path produced by the collect step, including a runtime-generated filename when the final archive name is not known in advance.

#### Scenario: Collect generates the archive filename at runtime
- **GIVEN** the collect workflow determines the final archive filename during execution
- **WHEN** the user runs `esdiag collect <host> <output> --upload <upload_id>`
- **THEN** the system resolves the final emitted archive path from the completed collect step
- **AND** the upload handoff uses that resolved archive path instead of requiring the user to supply the generated filename

#### Scenario: Collect fails before producing an archive
- **GIVEN** the user runs `esdiag collect <host> <output> --upload <upload_id>`
- **WHEN** the collect step fails before producing a diagnostic archive
- **THEN** the command returns the collect failure
- **AND** the system does not attempt the upload handoff

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

### Requirement: Collection Definition Covers API-Collectable Products Only
The collection definition registry (`assets/<product>/sources.yml`) SHALL describe API
sources only for the API-collectable products (Elasticsearch, Kibana, Logstash). The
system SHALL NOT define or resolve API sources for Elastic Agent or any platform.

#### Scenario: No API source set for a product-provided product
- **WHEN** the collect list is resolved for a run
- **THEN** the system resolves API sources only from the Elasticsearch, Kibana, or Logstash collection definitions
- **AND** no API source set exists for Elastic Agent or any platform

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
