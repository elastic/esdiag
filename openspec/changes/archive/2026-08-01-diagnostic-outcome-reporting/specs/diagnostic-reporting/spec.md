## ADDED Requirements

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
The persisted `DiagnosticReport` SHALL be the single source of truth for the diagnostic
verdict. The owner-scoped job feed SHALL render collection and processing failures from
the persisted report's recorded events, and the CLI exit code and WebUI status SHALL be
determined from the same single `DiagnosticOutcome`. No consumer SHALL derive the
verdict from a separate path.

#### Scenario: Job feed renders recorded failures
- **WHEN** the owner-scoped job feed displays a completed diagnostic that recorded
  failure events
- **THEN** the feed MUST render those failures from the persisted report

#### Scenario: CLI exit code reflects the outcome
- **WHEN** the CLI `process` command finishes a diagnostic
- **THEN** its exit code MUST be determined by the report's `DiagnosticOutcome`

#### Scenario: WebUI status reflects the outcome
- **WHEN** the WebUI shows the status of a completed diagnostic
- **THEN** the displayed status MUST be the report's single `DiagnosticOutcome`

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
