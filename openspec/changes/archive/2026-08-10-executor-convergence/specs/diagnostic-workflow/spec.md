## ADDED Requirements

### Requirement: Workflow Draft Compiles to Unified Job Phases
The web workflow SHALL hold editable panel state as a backend-owned `JobDraft`
whose fields mirror the unified phases (`input`, `save`, `process` with its
export sink, `send`). Datastar signals SHALL carry form interaction values, not
an independently executable operation model. On execution, the backend SHALL
compile the draft into one validated `Job` and its runtime execution bindings.
Incomplete draft state MAY be represented while editing but SHALL NOT be
treated as an executable `Job`. The UI verbs `collect` / `process` / `send`
remain presentation labels and SHALL NOT be required to map one-to-one onto
backend stages.

#### Scenario: Panel selections construct a Job
- **WHEN** the user configures the `Collect`, `Process`, and `Send` panels and executes
- **THEN** the backend MUST compile the JobDraft into a single validated Job and its execution bindings
- **AND** it MUST hand that Job and context to the one executor

#### Scenario: Invalid stage combination is rejected before execution
- **WHEN** the configured panels would produce a `Job` that violates a construction invariant (e.g. `Send` with no bundle)
- **THEN** the workflow MUST reject the configuration before execution rather than start an invalid run

#### Scenario: Incomplete form state remains a draft
- **GIVEN** the user has not yet selected a required host or output target
- **WHEN** the backend receives the current form signals
- **THEN** it MUST preserve the editable JobDraft state
- **AND** it MUST NOT weaken Job construction invariants to represent that incomplete state

### Requirement: Processed and Raw Output Targets Are Distinct
The JobDraft SHALL store the processed-document Export target independently
from the raw-bundle Send target. Converting a Job to editable state and back
SHALL preserve both targets without one overwriting the other.

#### Scenario: Draft round-trips both remote targets
- **GIVEN** a staged Job exports processed documents to a diagnostic cluster and sends its raw bundle to an upload-service target
- **WHEN** the workflow projects that Job into editable draft state and recompiles it
- **THEN** the diagnostic-cluster Export target MUST remain selected
- **AND** the upload-service Send target MUST remain selected

### Requirement: Existing Bundle Retention Is Execution Policy
The workflow SHALL configure retained-input execution policy, rather than a
Save stage, when the selected web input is an existing remote bundle such as an
Elastic Upload Service link and the user requests a retained browser download.
`Save` SHALL remain the stage that serializes a newly collected diagnostic.

#### Scenario: Service-link input is retained and processed
- **GIVEN** the Collect panel source resolves to an existing service-link bundle
- **AND** the user requests a retained download and processing
- **WHEN** the draft compiles
- **THEN** the Job MUST use Load plus Process/Export without Save
- **AND** the execution context MUST retain the materialized input for browser download

## MODIFIED Requirements

### Requirement: Three-Panel Diagnostic Workflow
The web advanced workflow pages SHALL present distinct panels named `Collect`,
`Process`, and `Send`. `Collect` SHALL provide `Collect` or `Upload`, and
`Process` SHALL provide `Process` or `Forward`. The `Send` panel SHALL expose
the output roles compatible with the current draft: a processed-document
target when Process is selected and a raw-bundle target when a bundle exists.
Each visible output role SHALL offer its compatible Remote and Local choices
independently. The workflow SHALL preserve draft state across user interaction
so source, processing, and both delivery targets can be configured together
before execution.

#### Scenario: User loads an advanced workflow page
- **WHEN** an advanced workflow page is rendered
- **THEN** the interface shows separate Collect, Process, and Send panels in the primary workflow area
- **AND** Collect and Process expose their two stage options
- **AND** Send exposes only the output roles and controls compatible with the current draft

#### Scenario: Processed and raw targets choose different locality
- **GIVEN** Process is selected and a bundle exists
- **WHEN** the user configures the Send panel
- **THEN** the processed-document target MAY use a Local choice
- **AND** the raw-bundle target MAY independently use a Remote upload-service choice

### Requirement: Send Stage Options
The `Send` panel SHALL present independent processed-document and raw-bundle
target groups when their preconditions are met. Each group SHALL support the
compatible `Remote` and `Local` choices: processed Remote targets a diagnostic
cluster, processed Local targets a localhost cluster or directory, raw Remote
targets Elastic Upload Service, and raw Local reuses retained browser download
rather than introducing a second filesystem target.

#### Scenario: User chooses processed remote output
- **WHEN** the user chooses Remote for the processed-document target
- **THEN** the panel displays diagnostic-cluster delivery inputs

#### Scenario: User chooses raw remote send
- **WHEN** the user chooses Remote for the raw-bundle target
- **THEN** the panel displays Elastic Upload Service delivery inputs

#### Scenario: User chooses local output
- **WHEN** the user chooses a Local option for an available output role
- **THEN** the panel displays the local behavior compatible with that role

### Requirement: Send Target Availability Follows Workflow State
The backend SHALL derive `Send` panel target availability from the active
JobDraft input, Save, and Process selections and patch the resulting form state
or elements over SSE. Targets whose preconditions cannot be met SHALL be
disabled and cleared before execution and SHALL NOT remain selectable until the
draft returns to a compatible state. Because Phase 3 is *and/or*, the
processed-document Export target and raw-bundle Send target are independent.
Both MAY be enabled when Process is selected and a bundle exists through Load
or Save.

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

#### Scenario: Loaded bundle enables both targets without Save
- **GIVEN** the workflow loads an existing bundle and is configured to process it
- **WHEN** the Send panel renders available delivery targets
- **THEN** the processed-output target MUST be enabled when otherwise valid
- **AND** the raw-bundle Send target MUST be enabled without adding a Save stage

#### Scenario: Live streaming collection cannot send a raw bundle
- **GIVEN** the workflow collects from live APIs and processes without Save
- **WHEN** the Send panel renders available delivery targets
- **THEN** the processed-output target MAY remain enabled
- **AND** the raw-bundle Send target MUST be disabled because no bundle exists

### Requirement: Remote Send Behavior
When remote delivery is selected, the workflow SHALL represent processed
diagnostic delivery as the Process stage's Export target and raw archive
delivery as the Send stage's Elastic Upload Service target. These targets SHALL
be stored and validated independently. A single staged Job MAY select both and
the executor SHALL attempt both independently once the bundle exists.

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

#### Scenario: One remote output fails
- **GIVEN** the workflow enables both a remote cluster Export target and an upload-service Send target
- **WHEN** one output fails after the bundle exists
- **THEN** the workflow MUST still attempt the other output
- **AND** it MUST present both stage results and a non-success aggregate status

### Requirement: Local Send Behavior
Processed diagnostics SHALL support local delivery to either a localhost
diagnostic cluster target or a local directory. Forwarded raw archives SHALL
NOT support a second local filesystem target; local raw delivery SHALL use the
Collect panel's retained browser download. For live Collect, selecting local
raw delivery SHALL enable retained Save. For an existing Load input, it SHALL
enable retained-input execution policy without adding Save.

#### Scenario: Processed local send targets localhost cluster
- **GIVEN** the workflow is configured for Process
- **WHEN** the user chooses a localhost diagnostic cluster for processed local delivery
- **THEN** the target MUST be valid only when the host resolves to `localhost` or `127.0.0.1`

#### Scenario: Processed local send targets directory
- **GIVEN** the workflow is configured for Process
- **WHEN** the user chooses directory delivery for the processed-document target
- **THEN** the workflow MUST write processed output to the selected local directory

#### Scenario: Live forward local send enables Save
- **GIVEN** the workflow forwards a live collection
- **WHEN** the user chooses local raw-bundle delivery
- **THEN** the UI MUST state that local bundle download is handled in Collect
- **AND** the workflow MUST enable retained Save if it is currently disabled

#### Scenario: Loaded forward local send retains input
- **GIVEN** the workflow forwards an existing uploaded or service-link bundle
- **WHEN** the user chooses local raw-bundle delivery
- **THEN** the UI MUST state that local bundle download is handled in Collect
- **AND** the workflow MUST enable retained-input policy without adding a Save stage
