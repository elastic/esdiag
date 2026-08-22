## ADDED Requirements

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

## MODIFIED Requirements

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
