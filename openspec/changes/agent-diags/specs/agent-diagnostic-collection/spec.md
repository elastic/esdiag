## ADDED Requirements

### Requirement: Agent Diagnostic Bundle Detection
The system SHALL detect Agent diagnostic bundles by looking for `agent-info.yaml` when neither `diagnostic_manifest.json` nor `manifest.json` is present. When found, the system MUST synthesize a `DiagnosticManifest` with `product: Agent`, version from `metadata.elastic.agent.version`, collection date parsed from the archive directory name, and name from `metadata.host.hostname`.

#### Scenario: Agent bundle is detected via agent-info.yaml
- **GIVEN** a diagnostic archive that contains `agent-info.yaml` but no `diagnostic_manifest.json` or `manifest.json`
- **WHEN** the receiver attempts to build a manifest
- **THEN** the system reads `agent-info.yaml` and constructs a `DiagnosticManifest` with `product: Agent`
- **AND** `version` is set from `metadata.elastic.agent.version`
- **AND** `name` is set from `metadata.host.hostname`

#### Scenario: Collection date is parsed from the archive directory name
- **GIVEN** an Agent diagnostic archive named `elastic-agent-diagnostics-YYYY-MM-DDTHH-MM-SSZ-NN`
- **WHEN** the manifest is synthesized
- **THEN** `collection_date` is set to the corresponding ISO-8601 timestamp

#### Scenario: Fallback collection date when directory name is unparseable
- **GIVEN** an Agent diagnostic archive whose directory name does not match the expected pattern
- **WHEN** the manifest is synthesized
- **THEN** `collection_date` falls back to the current system time

### Requirement: Agent Diagnostic Processing
The system SHALL process Agent diagnostic bundles by implementing `AgentDiagnostic` as a `DiagnosticProcessor`. It MUST be dispatched when the manifest identifies `product: Agent`.

#### Scenario: Agent bundle is processed end-to-end
- **GIVEN** a diagnostic archive whose manifest reports `product: agent`
- **WHEN** the processor dispatches on product type
- **THEN** `AgentDiagnostic::try_new` is called with the receiver and manifest
- **AND** `AgentDiagnostic::process` exports documents to Elasticsearch

### Requirement: Agent State Export
The system SHALL export `state.yaml` to `metrics-agent.state-esdiag`. The top-level agent state (fleet_state, message, state) SHALL produce one document. Each entry in `components[]` SHALL produce a separate document containing the component id, state, message, units, and version_info.

#### Scenario: State is split into agent and component documents
- **GIVEN** a valid Agent bundle with a `state.yaml` containing 5 components
- **WHEN** `AgentDiagnostic::process` runs
- **THEN** 6 documents are indexed to `metrics-agent.state-esdiag`: 1 agent-level + 5 component-level

### Requirement: Computed Config Export
The system SHALL export `computed-config.yaml` as a single document to `settings-agent.computed_config-esdiag` using opaque value passthrough (no schema modeling).

#### Scenario: Computed config is exported as a single document
- **GIVEN** a valid Agent bundle containing `computed-config.yaml`
- **WHEN** `AgentDiagnostic::process` runs
- **THEN** one document is indexed to `settings-agent.computed_config-esdiag`

### Requirement: Local Config Export
The system SHALL export `local-config.yaml` as a single document to `settings-agent.local_config-esdiag`.

#### Scenario: Local config is exported
- **GIVEN** a valid Agent bundle containing `local-config.yaml`
- **WHEN** `AgentDiagnostic::process` runs
- **THEN** one document is indexed to `settings-agent.local_config-esdiag`

### Requirement: Per-Component Beat Metrics Export
The system SHALL dynamically discover `components/*/` subdirectories and export each component's `beat_metrics.json` to `metrics-agent.beat-esdiag`. Each document SHALL have a `component` field injected with the subdirectory name (e.g. `"cel-default"`, `"aws-s3-default"`).

#### Scenario: Beat metrics are exported for each component
- **GIVEN** a valid Agent bundle with 5 component subdirectories each containing `beat_metrics.json`
- **WHEN** `AgentDiagnostic::process` runs
- **THEN** 5 documents are indexed to `metrics-agent.beat-esdiag`, each with a `component` field set to the subdirectory name

### Requirement: Consolidated Input Metrics Export
The system SHALL collect `input_metrics.json` from all `components/*/` subdirectories and export every array entry to a single `metrics-agent.input-esdiag` data stream. Each entry self-identifies via its `input` field (component type, e.g. `"cel"`, `"aws-s3"`) and `id` field (stream ID). No additional `component` enrichment is required.

#### Scenario: Input metrics from all components are consolidated
- **GIVEN** a valid Agent bundle with 3 component subdirectories, containing 1, 3, and 2 input metric entries respectively
- **WHEN** `AgentDiagnostic::process` runs
- **THEN** 6 documents are indexed to `metrics-agent.input-esdiag`
- **AND** each document retains its original `input` and `id` fields identifying the source component and stream

### Requirement: Component Field Alias on Input Metrics
The `metrics-agent.input-esdiag` data stream mapping SHALL define a field alias `component` that points to the `input` field. This allows a single `component` field filter to work across both `metrics-agent.beat-esdiag` (where `component` is a concrete field) and `metrics-agent.input-esdiag` (where it aliases `input`).

#### Scenario: Filtering by component works across both data streams
- **GIVEN** documents in `metrics-agent.beat-esdiag` with `component: "cel-default"` and documents in `metrics-agent.input-esdiag` with `input: "cel"`
- **WHEN** a user queries both data streams filtering on `component`
- **THEN** the alias resolves `component` to `input` on the input data stream
- **AND** both beat and input metric documents for the matching component are returned

### Requirement: Rendered Config Split by Top-Level Key
The system SHALL parse each component's `beat-rendered-config.yml` and route each top-level key to a dedicated data stream. The known top-level keys are `inputs`, `outputs`, `features`, and `apm`. Each document SHALL be enriched with a `component` field set to the subdirectory name.

#### Scenario: Inputs are exported per-entry
- **GIVEN** a component `cel-default` with a `beat-rendered-config.yml` containing 1 input entry
- **WHEN** `AgentDiagnostic::process` runs
- **THEN** 1 document is indexed to `settings-agent.inputs-esdiag` with `component: "cel-default"`

#### Scenario: Outputs are exported per component
- **GIVEN** a valid Agent bundle with 5 component subdirectories each containing an `outputs` key
- **WHEN** `AgentDiagnostic::process` runs
- **THEN** 5 documents are indexed to `settings-agent.outputs-esdiag`, one per component

#### Scenario: Features and APM are exported per component
- **GIVEN** a component with `features` and `apm` keys in its rendered config
- **WHEN** `AgentDiagnostic::process` runs
- **THEN** 1 document is indexed to `settings-agent.features-esdiag` and 1 to `settings-agent.apm-esdiag`

### Requirement: Flattened Field Type for Deep Nesting
For rendered config data streams, any nested arrays or objects deeper than the first level SHALL use the Elasticsearch `flattened` field type. This prevents mapping explosions from deeply nested or variable-schema config structures (e.g. `processors`, `streams`, `request.transforms`).

#### Scenario: Deeply nested input config uses flattened fields
- **GIVEN** an input entry in `beat-rendered-config.yml` with nested `processors` and `streams` arrays
- **WHEN** the document is exported to `settings-agent.inputs-esdiag`
- **THEN** the `processors` and `streams` fields are stored using the `flattened` field type
- **AND** first-level fields (`type`, `id`, `data_stream`, `index`) remain as standard mapped fields

### Requirement: Log Forwarding
The system SHALL forward all `logs/**/*.ndjson` files to `logs-elastic.agent-esdiag`, EXCEPT files under `events/` directories. Each NDJSON line SHALL be enriched with `agent.*` and `diagnostic.*` metadata fields. The original `@timestamp` in each line SHALL be preserved.

#### Scenario: Agent logs are forwarded with metadata
- **GIVEN** a valid Agent bundle with 8 agent log files under `logs/data/`
- **WHEN** `AgentDiagnostic::process` runs
- **THEN** each NDJSON line from each file is indexed to `logs-elastic.agent-esdiag`
- **AND** each line includes `agent.id`, `agent.version`, `diagnostic.id`, and `diagnostic.collection_date`
- **AND** the original `@timestamp` from the log line is preserved

### Requirement: Event Log Exclusion
The system SHALL NOT ingest NDJSON files located under `events/` directories within the logs hierarchy (matching `logs/**/events/*.ndjson` and `logs/data/events/*.ndjson`). These files contain sensitive operational data that MUST be excluded from diagnostic processing.

#### Scenario: Event logs are excluded from forwarding
- **GIVEN** a valid Agent bundle with files under `logs/data/events/`
- **WHEN** `AgentDiagnostic::process` enumerates log files for forwarding
- **THEN** no files from `logs/data/events/` are forwarded
- **AND** no documents from event log files appear in `logs-elastic.agent-esdiag`

#### Scenario: Non-event logs in sibling directories are still forwarded
- **GIVEN** a valid Agent bundle with files under both `logs/data/` and `logs/data/events/`
- **WHEN** `AgentDiagnostic::process` enumerates log files
- **THEN** files directly under `logs/data/` (e.g. `elastic-agent-*.ndjson`) are forwarded
- **AND** files under `logs/data/events/` are excluded

### Requirement: Agent Metadata Enrichment
Every exported document SHALL include `agent.*` fields (id, version, snapshot, unprivileged), `host.*` fields (hostname, name, arch, ip), `os.*` fields (family, name, platform, version, kernel), and `diagnostic.*` fields (id, uuid, collection_date, product).

#### Scenario: Metadata is embedded in exported documents
- **GIVEN** a valid Agent bundle
- **WHEN** any data source exports documents
- **THEN** each document contains `agent.id`, `agent.version`, `host.hostname`, `os.name`, `diagnostic.id`, and `diagnostic.collection_date`

### Requirement: Missing Optional Source Tolerance
The system SHALL tolerate missing optional data source files. Only `agent-info.yaml` is required. All other files (state, configs, component files, logs) SHALL be optional — if absent, the processor logs a warning and continues.

#### Scenario: Missing optional source is tolerated
- **GIVEN** an Agent bundle that does not contain `state.yaml`
- **WHEN** `AgentDiagnostic::process` runs
- **THEN** the processor logs a warning and continues without failing
- **AND** the `ProcessorSummary` records zero documents for the state source

### Requirement: Agent Processor Origin
The system SHALL implement `origin()` on `AgentDiagnostic` returning the agent hostname, agent ID, and the literal role string `"agent"`.

#### Scenario: Origin is recorded in the diagnostic report
- **GIVEN** processing completes for an Agent bundle
- **WHEN** the diagnostic report is finalised
- **THEN** the report origin contains the agent hostname, agent ID, and role `"agent"`
