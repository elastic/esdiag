## ADDED Requirements

### Requirement: Complete Kibana Source Discovery
The system SHALL process every available file for a selected Kibana source across root, space-scoped, paginated, and legacy numbered layouts.

#### Scenario: Scoped and paginated source processing
- **WHEN** a selected source has files under `spaces/<space>`, `pages/page-*`, or both
- **THEN** the system processes every matching file exactly once in deterministic path order

#### Scenario: Legacy numbered source processing
- **WHEN** a diagnostic contains a legacy source file such as `kibana_alerts_1.json`
- **THEN** the system processes the numbered file as an instance of its canonical source

### Requirement: Kibana Processing Selection
The system SHALL expose implemented Kibana sources through the processing registry and honor the resolved processing selection.

#### Scenario: Selected source dispatch
- **WHEN** processing is requested with an explicit Kibana source selection
- **THEN** the system processes required sources and selected optional sources without processing unselected optional sources

### Requirement: Kibana Processing Outcomes
The system SHALL distinguish missing optional sources from successfully parsed sources and processing failures.

#### Scenario: Successful source outcome
- **WHEN** a Kibana source is decoded and exported without document rejection
- **THEN** the report records the source as parsed and does not make the diagnostic outcome partial

#### Scenario: Source failure outcome
- **WHEN** a present Kibana source cannot be read, parsed, or exported
- **THEN** the report records an actionable processing event and derives the diagnostic outcome from that event

### Requirement: Kibana Output Templates
The system SHALL install an index template matching every emitted Kibana metrics data stream.

#### Scenario: Dynamic Kibana payload mapping
- **WHEN** a Kibana metrics document contains a dashboard-critical metadata field and version-dependent payload fields
- **THEN** the template explicitly maps the dashboard metadata and dynamically maps the remaining payload
