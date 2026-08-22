# Diagnostic Reporting

## Purpose

Defines the `DiagnosticReport`: the events it records during collection and processing,
the single outcome derived from them, and how consumers — the CLI exit code, the web job
feed, and Kibana links — read that one verdict.
## Requirements
### Requirement: Record parsing status for lookups
The system SHALL record the `parsed` status for every entry in the `lookup` section of the `DiagnosticReport`.

#### Scenario: Successful lookup
- **WHEN** a lookup table is successfully populated (marked as `parsed: true`)
- **THEN** the corresponding entry in the `lookup` section of the report has `parsed: true`

#### Scenario: Failed lookup
- **WHEN** a lookup table fails to be populated (marked as `parsed: false`)
- **THEN** the corresponding entry in the `lookup` section of the report has `parsed: false`

### Requirement: Record lookup failures in summary
The system SHALL track the total number of lookup failures and the names of failed lookups.

#### Scenario: Failure tracking
- **WHEN** `add_lookup` is called with a lookup that was not successfully parsed
- **THEN** `diagnostic.lookup.errors` is incremented
- **AND** the lookup name is added to `diagnostic.lookup.failures`

### Requirement: Graceful handling of missing enrichment metadata
The processing pipeline SHALL handle missing enrichment metadata (such as node information for tasks) gracefully, without causing the application to panic or terminate diagnostic processing.

#### Scenario: Missing node metadata for a task
- **WHEN** the task processor attempts to enrich a task with node metadata
- **AND** the node ID for that task is not found in the node lookup table
- **THEN** the system SHALL log an error or warning message
- **AND** the system SHALL continue to process and export the task document without node metadata

### Requirement: Viewer-Aware Kibana Link Selection
The system SHALL determine the Kibana base URL for final processed-diagnostic reporting by first resolving the explicit output target's saved viewer host, and SHALL fall back to `ESDIAG_KIBANA_URL` when no saved viewer host is available. If `ESDIAG_KIBANA_SPACE` is present, the system SHALL append the configured space path to the selected Kibana base URL before constructing the final Kibana link.

#### Scenario: Saved viewer host overrides environment Kibana URL
- **GIVEN** a processed diagnostic is sent to a saved Elasticsearch host with role `send`
- **AND** that saved host references a saved Kibana viewer host
- **AND** `ESDIAG_KIBANA_URL` is also set
- **WHEN** final processing reporting builds the Kibana link
- **THEN** the link uses the saved viewer host URL as its base URL

#### Scenario: Environment fallback is used when no saved viewer host exists
- **GIVEN** a processed diagnostic completes without a resolved saved viewer host
- **AND** `ESDIAG_KIBANA_URL` is set
- **WHEN** final processing reporting builds the Kibana link
- **THEN** the link uses `ESDIAG_KIBANA_URL` as its base URL

#### Scenario: Default Kibana URL is used when no override source is available
- **GIVEN** a processed diagnostic completes without a resolved saved viewer host
- **AND** `ESDIAG_KIBANA_URL` is not explicitly set
- **WHEN** final processing reporting completes
- **THEN** the link uses the default Kibana base URL

### Requirement: First-class diagnostic outcome
Every `DiagnosticReport` SHALL carry exactly one `DiagnosticOutcome` drawn from the
closed set `Complete | Partial | Failed | Skipped`. The outcome applies to **any**
diagnostic — parent or child alike — and SHALL be **derived** from the events recorded
in that report, never assigned independently of them. Derivation MUST follow: any
recorded failure or partial-capture event → `Partial`; a total failure where nothing
was collected or processed → `Failed`; an unsupported diagnostic → `Skipped`;
otherwise → `Complete`.

#### Scenario: All sources succeed
- **WHEN** a diagnostic completes with every collected and processed source recording
  only success-level events
- **THEN** the report's `DiagnosticOutcome` MUST be `Complete`

#### Scenario: Some sources fail
- **WHEN** a diagnostic completes with at least one source recording a
  failure or partial-capture event while others succeed
- **THEN** the report's `DiagnosticOutcome` MUST be `Partial`

#### Scenario: Nothing is collected or processed
- **WHEN** a diagnostic records no successful capture and at least one total failure
- **THEN** the report's `DiagnosticOutcome` MUST be `Failed`

#### Scenario: Outcome matches recorded events
- **WHEN** a report is persisted
- **THEN** its `DiagnosticOutcome` MUST equal the value derived from that report's own
  recorded events, with no way to persist an outcome inconsistent with them

### Requirement: Record all diagnostic events
The `DiagnosticReport` SHALL record every error-, warning-, and success-level event
that occurs during collection and processing, and each recorded event MUST carry its
**source** (the data source, processor, or exporter it came from) and a **reason**.
Collection failures and processing failures MUST both be recorded as events; a failure
MUST NOT be dropped to a tracing log in place of being recorded. In particular, when a
per-source processor result is an error (the `ProcessorSummary` merge of an `Err`), the
system SHALL record a failure event carrying the source and reason instead of only
emitting a `tracing::warn!`.

#### Scenario: Processor source fails
- **WHEN** processing a source yields an error that is merged into the report
- **THEN** the report MUST record a failure event for that source with its reason
- **AND** the failure MUST NOT be represented only by a tracing log line

#### Scenario: Collection failure is recorded
- **WHEN** a source cannot be collected
- **THEN** the report MUST record a failure event identifying that source and the
  reason it could not be collected

#### Scenario: Successful source is recorded
- **WHEN** a source is collected and processed successfully
- **THEN** the report MUST record a success-level event identifying that source

### Requirement: Report is the persisted source of truth
The persisted `DiagnosticReport` SHALL remain the single source of truth for a
diagnostic's verdict, and its `DiagnosticOutcome` SHALL be derived only from the
report's recorded events, including per-source exporter transport and document
results. `ExecutionOutcome` SHALL be the source of truth for the whole Job's
terminal status across input, Save, Process, Export, and Send. A stage failure
outside the report event stream SHALL NOT imperatively rewrite a completed
diagnostic's `DiagnosticOutcome`; an exporter failure recorded in the report
SHALL affect the derived verdict under the existing outcome rules. Any
hard-failed selected parent stage SHALL make the aggregate Job status
non-success. Consumers SHALL display the diagnostic verdict and whole-job
status distinctly when they differ.

#### Scenario: Job feed renders recorded diagnostic failures
- **WHEN** the owner-scoped job feed displays a completed diagnostic that recorded failure events
- **THEN** it MUST render those diagnostic failures from the persisted report

#### Scenario: CLI exit code reflects diagnostic and execution outcomes
- **WHEN** the CLI process command finishes a Job
- **THEN** its exit code MUST be non-zero when the report's DiagnosticOutcome is Failed
- **AND** it MUST be non-zero when any selected parent stage has failed
- **AND** it MUST still render every successful stage result

#### Scenario: Web UI distinguishes verdict from job status
- **GIVEN** Process completes with a successful DiagnosticOutcome
- **AND** a selected raw-bundle Send fails
- **WHEN** the Web UI renders completion
- **THEN** the diagnostic verdict MUST remain the report's successful DiagnosticOutcome
- **AND** the Job terminal status MUST be non-success with the Send failure shown

#### Scenario: Export failure preserves completed report
- **GIVEN** Process produces a completed DiagnosticReport
- **WHEN** Export fails
- **THEN** the report and its DiagnosticOutcome derived from all recorded exporter events MUST remain available
- **AND** the ExecutionOutcome MUST record Export failure separately

#### Scenario: Document rejection affects diagnostic verdict
- **WHEN** Export records rejected documents or transport failures in the DiagnosticReport
- **THEN** the DiagnosticOutcome MUST reflect those recorded events
- **AND** the executor MUST NOT replace that derived verdict with a separately assigned value

### Requirement: Two-level export status
Export status SHALL be recorded at two distinct levels: the **request/transport** status
code of a single call, and the **document-level** status of individual documents within
it. A request MAY succeed at transport level (for example an Elasticsearch `_bulk`
request returning `200`) while individual documents are rejected (for example per-doc
`409` or `429`). The per-document `status_counts` histogram SHALL be authoritative for
document outcomes and SHALL feed the `Partial` verdict; the scalar request status code
MUST NOT collapse a set of mixed document statuses into a single value.

#### Scenario: Bulk request succeeds but documents are rejected
- **WHEN** a `_bulk` request returns transport status `200` but the response reports
  per-document `409` conflicts
- **THEN** the `status_counts` histogram MUST record the per-document `409` outcomes
- **AND** the derived `DiagnosticOutcome` MUST be `Partial`

#### Scenario: Document histogram is authoritative
- **WHEN** the request status code and the `status_counts` histogram disagree about
  document success
- **THEN** the document outcomes MUST be taken from `status_counts`

### Requirement: Reserved non-HTTP exporter status
The request status code `0` SHALL be reserved to denote a non-HTTP exporter — file,
stream, or directory — that has no HTTP transport status. Status `0` SHALL NOT be used
to represent a mixed set of HTTP request codes; merging results with differing HTTP
request codes MUST NOT collapse them to `0`.

#### Scenario: File exporter reports status 0
- **WHEN** a file, stream, or directory exporter records a result
- **THEN** its request status code MUST be `0`

#### Scenario: Mixed HTTP codes are not collapsed to 0
- **WHEN** results with differing HTTP request status codes are merged
- **THEN** the merged result MUST NOT report request status `0`

### Requirement: Skipped distinguishes by-design from not-implemented
A `Skipped` outcome SHALL distinguish a **by-design** skip — a diagnostic that is out
of scope, such as platform-level API collection — from a **not-implemented** skip —
work that is planned but not yet built, such as Kibana or Agent processing. The report
SHALL carry which kind of skip occurred so that a skip reads as "nothing to do here"
versus "TODO" rather than an undifferentiated non-result.

#### Scenario: By-design skip
- **WHEN** a diagnostic is skipped because it is out of scope by design
- **THEN** the report's `Skipped` outcome MUST indicate the skip was by-design

#### Scenario: Not-implemented skip
- **WHEN** a diagnostic is skipped because its processing is recognized but not yet
  implemented
- **THEN** the report's `Skipped` outcome MUST indicate the skip was not-implemented
