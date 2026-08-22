## Purpose

Describe reporting behavior for ECK and KubernetesPlatform parent diagnostic bundles that fan out into included child diagnostics.
## Requirements
### Requirement: Included Diagnostic Job Fan-Out Reporting
When a web processing job receives an ECK or KubernetesPlatform parent diagnostic bundle with `included_diagnostics`, the system SHALL report each included diagnostic as a distinct child job in the web job feed.

#### Scenario: Parent bundle starts multiple child jobs
- **WHEN** a web processing job starts for an ECK or KubernetesPlatform parent bundle whose manifest lists multiple `included_diagnostics`
- **THEN** the job feed MUST display a separate progress box for each included diagnostic that esdiag starts processing
- **AND** each child progress box MUST identify the child diagnostic source or path separately from the parent bundle source

#### Scenario: Parent bundle does not hide child work
- **WHEN** the parent ECK or KubernetesPlatform processor completes without producing orchestration-level documents
- **THEN** the job feed MUST preserve the parent result
- **AND** the job feed MUST show child diagnostic job results when child outcomes exist

### Requirement: Child Diagnostic Completion Links
Each successfully processed included diagnostic SHALL be reported with its own diagnostic metadata and Kibana link.

#### Scenario: Supported child diagnostic completes
- **WHEN** an included Elasticsearch diagnostic completes successfully
- **THEN** the child job result MUST display that child report's `diagnostic.id`
- **AND** the child job result MUST link to that child report's Kibana URL when a Kibana base URL is configured
- **AND** the child job result MUST display the child report's product, created document count, and processing duration

#### Scenario: Multiple supported children complete
- **WHEN** multiple included Elasticsearch diagnostics complete from the same parent bundle
- **THEN** the job feed MUST display one completed result per child diagnostic
- **AND** each completed result MUST use the `diagnostic.id` and Kibana link from its own child report

#### Scenario: CLI process returns structured child outcomes
- **WHEN** the CLI `process` command completes an ECK or KubernetesPlatform parent bundle with one or more successfully processed child diagnostics
- **THEN** the structured process outcome MUST include one completed child entry per diagnostic
- **AND** each entry MUST contain its `diagnostic.id`, product, created document count, integer duration in milliseconds, and Kibana URL when configured
- **AND** the outcome MUST NOT present the empty parent diagnostic link as the only actionable result

### Requirement: Synchronous API Multi-Result Reporting
Synchronous diagnostic processing APIs SHALL return one JSON array entry for each diagnostic result produced by processing.

#### Scenario: API returns parent and child diagnostic results
- **WHEN** synchronous `/api/api_key` or `/api/service_link` processing completes an ECK or KubernetesPlatform parent bundle with included diagnostic outcomes
- **THEN** the HTTP 200 response body MUST be a JSON array
- **AND** the array MUST contain an entry for the parent diagnostic
- **AND** the array MUST contain one entry for each included diagnostic outcome

#### Scenario: API successful diagnostic entry
- **WHEN** a parent or child diagnostic processes successfully in a synchronous API request
- **THEN** that diagnostic result entry MUST include `status: "success"`
- **AND** that diagnostic result entry MUST include `diagnostic_id`, `kibana_link`, and `took`

#### Scenario: API child failure entry does not fail parent response
- **WHEN** parent processing succeeds but an included diagnostic fails
- **THEN** the synchronous API response MUST remain HTTP 200
- **AND** the failed child diagnostic result entry MUST include `status: "failed"` and an error message
- **AND** the parent diagnostic result entry MUST remain present

### Requirement: Unsupported Included Diagnostic Info Results
Recognized included diagnostics without an implemented diagnostic processor SHALL be reported as informational skipped child results rather than hidden or failed parent work.

#### Scenario: Unsupported child diagnostic is recognized
- **WHEN** an included diagnostic manifest is readable but its product does not have an implemented processor
- **THEN** the job feed MUST display an `info` status result for that child diagnostic
- **AND** the result MUST explain that the child diagnostic was recognized but skipped because processing is not implemented

#### Scenario: CLI process returns structured skipped child
- **WHEN** the CLI `process` command reads an included diagnostic manifest whose product does not have an implemented processor
- **THEN** the structured process outcome MUST include a skipped child entry
- **AND** the entry MUST contain the source path, product when known, and skip reason

#### Scenario: API reports unsupported child diagnostic
- **WHEN** synchronous API processing reads an included diagnostic manifest whose product does not have an implemented processor
- **THEN** the API result array MUST include an entry for that child diagnostic with `status: "info"`
- **AND** the entry MUST explain that the child diagnostic was recognized but skipped because processing is not implemented

#### Scenario: Unsupported children do not block supported children
- **WHEN** a parent bundle contains both supported Elasticsearch child diagnostics and recognized unsupported child diagnostics
- **THEN** supported child diagnostics MUST still process and render completed results
- **AND** unsupported child diagnostics MUST render informational skipped results

### Requirement: Failed Included Diagnostics Remain Typed CLI Children
A failed included diagnostic SHALL appear as a failed child entry inside an otherwise successful parent CLI process outcome. The entry SHALL contain the source path and safe error message and MUST NOT convert the entire parent command into a failed outcome when the parent processing lifecycle completed successfully.

#### Scenario: Child fails after parent succeeds
- **GIVEN** parent diagnostic processing completes successfully
- **WHEN** one included diagnostic fails
- **THEN** the process command exits successfully with a structured parent outcome
- **AND** the failed child appears with a failed discriminator, source path, and safe error message
- **AND** completed or skipped sibling outcomes remain present

### Requirement: Child Outcome Preservation
The execution lifecycle SHALL preserve a child `ExecutionOutcome` for each
included diagnostic. When a child produces a report, its `DiagnosticOutcome`
SHALL be derived from that child's recorded report events exactly as the
parent's is. Child stage status SHALL remain distinct: a child may have a
`Complete` diagnostic report and a non-success execution status when its Export
hard-fails. A child skipped before reporting SHALL retain whether the skip was
by-design or not-implemented. The parent `ExecutionOutcome` SHALL expose the
parent report and every child execution result. A child hard failure SHALL NOT
fail a successfully completed parent Process stage or the parent's aggregate
terminal status.

#### Scenario: Executor completes parent with child outcomes
- **WHEN** the executor completes an ECK or KubernetesPlatform parent Job with included diagnostics
- **THEN** the ExecutionOutcome MUST expose the parent diagnostic report
- **AND** it MUST expose a child ExecutionOutcome for every included diagnostic that was started, skipped, or failed
- **AND** each child report that exists MUST retain its derived DiagnosticOutcome

#### Scenario: Child failure does not fail completed parent
- **WHEN** parent processing succeeds and a child Job fails
- **THEN** the parent ExecutionOutcome MUST expose the child's non-success execution result
- **AND** the parent Process stage and aggregate terminal status MUST remain successful

#### Scenario: Child Export failure preserves diagnostic verdict
- **GIVEN** a child Process produces a report with DiagnosticOutcome Complete
- **WHEN** the child Export hard-fails
- **THEN** the child DiagnosticOutcome MUST remain Complete
- **AND** the child ExecutionOutcome MUST record Export failure
- **AND** the parent aggregate terminal status MUST remain successful

#### Scenario: Synchronous API preserves parent success
- **WHEN** a synchronous API request completes parent processing but a child Job fails
- **THEN** the response MUST remain HTTP 200
- **AND** the response array MUST contain the successful parent and failed child entries

#### Scenario: Child completes with partial captures
- **WHEN** an included Elasticsearch diagnostic processes but at least one of its sources records a failure or partial-capture event
- **THEN** the child's DiagnosticOutcome MUST be Partial
- **AND** the parent Process stage MUST still complete successfully

#### Scenario: Parent with skipped or no children succeeds
- **WHEN** an ECK or KubernetesPlatform parent diagnostic has no included diagnostics or all included diagnostics are skipped
- **THEN** the parent Process stage and aggregate terminal status MUST remain successful

#### Scenario: Child report keeps parent relationship
- **WHEN** a child diagnostic is processed from a parent bundle
- **THEN** the child diagnostic report MUST retain the parent diagnostic relationship metadata required by the orchestration metadata capability

#### Scenario: Included diagnostic reporting remains one level deep
- **WHEN** a child diagnostic contains its own included diagnostics
- **THEN** this capability MUST NOT require recursive multi-level reporting

### Requirement: Default Child Processing Selection
Included diagnostics SHALL process with their default product processor selection.

#### Scenario: Parent processing has included diagnostics with different products
- **WHEN** an ECK or KubernetesPlatform parent bundle contains included diagnostics for one or more products
- **THEN** each included diagnostic MUST use its default product processing selection
- **AND** parent-level process selection MUST NOT be applied as a filter to child diagnostics

### Requirement: Product-Provided Diagnostics Enter Via Load
The system SHALL acquire product-provided diagnostics via `Load`, never `Collect`.
Product-provided diagnostics are Elastic Agent (which generates its own bundle) and
every platform diagnostic (ECE, ECK, KubernetesPlatform, generated by the platform). A
job over such a diagnostic SHALL begin with
`Load` and take the shape `Load → [Process] → …`, and SHALL NOT include a `Collect`
stage. A platform bundle SHALL be Loaded and then fanned out into its included
diagnostics one level deep.

#### Scenario: Platform bundle job begins with Load
- **WHEN** a job processes an ECK or KubernetesPlatform diagnostic
- **THEN** the diagnostic MUST be acquired via `Load`
- **AND** the job MUST NOT include a `Collect` stage
- **AND** the parent bundle MUST fan out into its included diagnostics for processing

#### Scenario: Agent diagnostic job begins with Load
- **WHEN** a job processes an Elastic Agent diagnostic
- **THEN** the Agent-generated bundle MUST be acquired via `Load`
- **AND** the job MUST NOT include a `Collect` stage

#### Scenario: ECE bundle carries no application data
- **WHEN** an ECE diagnostic is Loaded
- **THEN** it MUST be treated as carrying no application data
- **AND** it MUST yield no included diagnostics to fan out

### Requirement: Recognized Unprocessable Child Is A Not-Yet-Implemented Gap
The system SHALL classify the skip of a recognized-but-unprocessable included diagnostic
as a **not-yet-implemented** gap (work in progress) — for example Kibana processing on a
branch, or Agent processing in progress (PR293). This classification SHALL be
distinguishable from the **out-of-scope-by-design** gap that governs the `Collect`
stage (Agent and platform-level API collection). The two SHALL NOT be conflated even
though both surface as a skip today. This classification ties to the `Skipped`
refinement in ADR-0016.

#### Scenario: Unprocessable child skip reads as work-in-progress
- **WHEN** a Loaded parent bundle includes a child diagnostic whose product has no implemented processor
- **THEN** the child MUST be reported as skipped because processing is not yet implemented
- **AND** this skip MUST be classified as not-yet-implemented, distinct from an out-of-scope-by-design boundary

#### Scenario: A product out of Collect scope may still be pending processing
- **WHEN** a Loaded diagnostic is an Elastic Agent bundle, a product ESDiag will never API-collect
- **THEN** the skip MUST still be classified as not-yet-implemented, because the by-design boundary governs collection and not processing
- **AND** each such application MUST carry its own reason rather than the shared unsupported-bundle reason, so the two classifications cannot merge

#### Scenario: By-design and not-yet-implemented gaps are not conflated
- **WHEN** the system reports why a diagnostic or child was not fully handled
- **THEN** an out-of-scope-by-design Collect refusal (Agent/platform API collection) MUST NOT be reported as not-yet-implemented
- **AND** a recognized-but-unprocessable child MUST NOT be reported as an out-of-scope-by-design boundary

### Requirement: Included Diagnostics Execute as Child Jobs
The system SHALL execute each included diagnostic of a parent bundle as a **child `Job`**
driven by the same executor. When the executor processes a parent bundle whose manifest lists
`included_diagnostics`, each included diagnostic SHALL be executed as a child `Job`:
a `Load`-input job over the nested diagnostic plus a `Process` stage and its
Export target. The child Load SHALL be a runtime binding to a path relative to
the parent's resolved bundle, not a fabricated standalone filesystem URI.
Child jobs SHALL reuse the one executor rather than a separate processing type.

#### Scenario: Parent spawns a child job per included diagnostic
- **WHEN** the executor processes an ECK or KubernetesPlatform parent bundle whose manifest lists included diagnostics
- **THEN** the executor MUST spawn one child `Job` for each included diagnostic
- **AND** each child `Job` MUST be a `Load`-input, `Process` job driven by the same executor

#### Scenario: Archive-backed child resolves relative to parent
- **GIVEN** a parent diagnostic is loaded from an archive
- **WHEN** an included diagnostic path is bound as a child Load input
- **THEN** the child execution context MUST resolve it relative to the parent receiver
- **AND** the child MUST NOT require that nested path to exist independently on the filesystem

### Requirement: Child Job Context Is Inherited Deliberately
Each child execution context SHALL mint a distinct child `JobID`, inherit the
parent owner and resolved `Platform`, preserve parent relationship metadata,
and reuse the parent's `DocumentExporter`. It SHALL set child depth to one.
Execution-only child bindings SHALL NOT be persistable as saved jobs.

#### Scenario: Child receives execution identity
- **WHEN** the parent creates a child Job
- **THEN** the child context MUST mint a JobID distinct from the parent and every sibling
- **AND** child lifecycle events MUST carry that child JobID and the inherited owner

#### Scenario: Child job inherits parent platform
- **WHEN** the parent spawns a child `Job` for an included Elasticsearch diagnostic
- **THEN** the child context MUST set the child's `Platform` from the parent before processing

#### Scenario: Child reuses document output
- **WHEN** a child Job processes an included diagnostic
- **THEN** it MUST export through the parent execution context's DocumentExporter
- **AND** it MUST retain metadata linking its report to the parent diagnostic

### Requirement: Child Job Outcomes Join the Parent Outcome
The executor SHALL include every started, skipped, completed, partial, or
failed child result in the parent's `ExecutionOutcome` and observer events.
A child failure SHALL NOT erase a successfully produced parent report or
prevent eligible sibling children from completing.

#### Scenario: One child fails while a sibling succeeds
- **GIVEN** a parent bundle contains two supported included diagnostics
- **WHEN** one child Job fails and the other completes
- **THEN** the parent ExecutionOutcome MUST contain both child results
- **AND** the successful parent report and successful sibling result MUST remain available

#### Scenario: Child job fan-out stays one level deep
- **WHEN** an included diagnostic processed as a child `Job` itself contains `included_diagnostics`
- **THEN** the executor MUST NOT recursively spawn grandchild jobs for that nested inclusion
- **AND** the child result MUST remain available without recursive work
