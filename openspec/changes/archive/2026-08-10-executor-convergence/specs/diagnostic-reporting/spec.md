## MODIFIED Requirements

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
