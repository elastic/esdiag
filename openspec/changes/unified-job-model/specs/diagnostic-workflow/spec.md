## ADDED Requirements

### Requirement: Workflow Binds to Unified Job Phases
The web workflow SHALL bind its panels directly to the unified `Job` phase model
(`input`, `save`, `process` with its export sink, `send`) rather than to a parallel
executable structure. The `JobSignals*` types SHALL be a thin presentation projection of a
`Job`, not an independent model. The UI verbs `collect` / `process` / `send` remain
presentation labels and SHALL NOT be required to map one-to-one onto the backend stages.

#### Scenario: Panel selections construct a Job
- **WHEN** the user configures the `Collect`, `Process`, and `Send` panels and executes
- **THEN** the workflow MUST construct a single `Job` from the panel selections and hand it to the executor
- **AND** the panel state MUST project the `Job` phases rather than a separate executable action structure

#### Scenario: Invalid stage combination is rejected before execution
- **WHEN** the configured panels would produce a `Job` that violates a construction invariant (e.g. `Send` with no bundle)
- **THEN** the workflow MUST reject the configuration before execution rather than start an invalid run

## MODIFIED Requirements

### Requirement: Send Target Availability Follows Workflow State
The `Send` panel SHALL derive target availability from the active `Collect` and `Process`
selections. Targets whose preconditions cannot be met by the current workflow state SHALL be
disabled before execution and SHALL NOT remain selectable until the workflow returns to a
compatible state. Because Phase 3 is *and/or*, a processed-output target and a raw-bundle
target are independent: when the workflow both processes and retains a bundle, both targets
MAY be enabled at once.

#### Scenario: Forward workflow disables processed send target
- **GIVEN** the workflow is configured to forward a collected or uploaded archive without processing
- **WHEN** the `Send` panel renders available delivery targets
- **THEN** targets intended for processed diagnostic output are disabled
- **AND** archive-compatible delivery targets remain enabled

#### Scenario: Processed workflow without a bundle disables archive send target
- **GIVEN** the workflow is configured to produce processed diagnostic output and does not retain a bundle
- **WHEN** the `Send` panel renders available delivery targets
- **THEN** archive-only delivery targets are disabled
- **AND** processed-output targets remain enabled when otherwise valid

#### Scenario: Processed workflow with a retained bundle enables both targets
- **GIVEN** the workflow is configured to produce processed diagnostic output and to retain the collected bundle
- **WHEN** the `Send` panel renders available delivery targets
- **THEN** the processed-output target remains enabled
- **AND** the raw-bundle send target is also enabled, so both may run in one job

### Requirement: Remote Send Behavior
When `Send -> Remote` is selected, the workflow SHALL send processed diagnostics to a
diagnostic cluster target and SHALL send raw archives to an Elastic Upload Service endpoint.
Because Phase 3 is *and/or*, a single job MAY do both — index processed output to a cluster
and forward the retained raw bundle to the upload service — in one run.

#### Scenario: Processed remote send targets diagnostic cluster
- **GIVEN** the workflow is configured for `Process -> Process`
- **WHEN** the user selects `Send -> Remote`
- **THEN** the workflow requires a remote diagnostic cluster target for processed output

#### Scenario: Forward remote send targets upload service
- **GIVEN** the workflow is configured for `Process -> Forward`
- **WHEN** the user selects `Send -> Remote`
- **THEN** the workflow requires an Elastic Upload Service endpoint
- **AND** the raw archive is forwarded unchanged

#### Scenario: Processed workflow also forwards the raw bundle
- **GIVEN** the workflow is configured to process output and to retain the collected bundle
- **WHEN** the user enables both a remote cluster target and an upload-service target
- **THEN** the workflow indexes the processed documents to the cluster
- **AND** in the same run forwards the retained raw bundle to the upload service
