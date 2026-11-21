# kibana-processing-runtime Specification

## Purpose
Define how Kibana diagnostic processing discovers scoped and paginated inputs, honors processing selections, reports processing outcomes, and installs compatible output templates.

## Requirements

### Requirement: Complete Kibana Source Discovery
The system SHALL process every available file for a selected Kibana source across root, space-scoped, paginated, and legacy numbered layouts.

#### Scenario: Scoped and paginated source processing
- **GIVEN** a selected source with files under `spaces/<space>`, `pages/page-*`, or both
- **WHEN** the bundle is processed
- **THEN** the system processes every matching file exactly once in deterministic path order

#### Scenario: Unrelated subtrees are not searched
- **GIVEN** a directory bundle that contains subdirectories outside the root, `spaces/<space>`, and `pages/page-*` layouts, including unreadable ones
- **WHEN** the bundle is processed
- **THEN** source discovery skips them and processes the selected sources that exist in the supported layouts

#### Scenario: Unreadable supported subtree
- **GIVEN** a bundle whose supported subtree, such as `spaces/<space>`, cannot be read
- **WHEN** the bundle is processed
- **THEN** the system processes the source files discovered elsewhere and records a processing event naming the unreadable subtree

#### Scenario: Archive entry order
- **GIVEN** a bundle archive whose first entry is a space-scoped or paginated file, at the archive root or under one top-level directory
- **WHEN** the bundle is processed
- **THEN** the system locates the bundle root from its manifest and processes the bundle as if the root files came first

#### Scenario: Legacy numbered source processing
- **GIVEN** a diagnostic that contains a legacy source file such as `kibana_alerts_1.json`
- **WHEN** the bundle is processed
- **THEN** the system processes the numbered file as an instance of its canonical source

### Requirement: Kibana Processing Selection
The system SHALL expose implemented Kibana sources through the processing registry and honor the resolved processing selection.

#### Scenario: Selected source dispatch
- **GIVEN** an explicit Kibana source selection
- **WHEN** processing is requested
- **THEN** the system processes required sources and selected optional sources without processing unselected optional sources

### Requirement: Kibana Processing Outcomes
The system SHALL distinguish missing optional sources from successfully parsed sources and processing failures.

#### Scenario: Successful source outcome
- **GIVEN** a Kibana source that decodes successfully
- **WHEN** it is exported without document rejection
- **THEN** the report records the source as parsed and does not make the diagnostic outcome partial

#### Scenario: Source failure outcome
- **GIVEN** a present Kibana source
- **WHEN** it cannot be read, parsed, or exported
- **THEN** the report records an actionable processing event and derives the diagnostic outcome from that event

#### Scenario: Version-inapplicable source
- **GIVEN** a selected Kibana source that the target Kibana version does not offer
- **WHEN** the bundle is processed
- **THEN** the system treats it as an unavailable optional source and does not make the diagnostic outcome partial

#### Scenario: Collection failure outcome
- **GIVEN** a diagnostic manifest that records a requested Kibana API without a successful HTTP status
- **WHEN** the bundle is processed
- **THEN** the report records a warning for that API, even if no source file was saved, and the diagnostic outcome is partial

### Requirement: Kibana Output Templates
The system SHALL install an index template matching every emitted Kibana metrics data stream.

#### Scenario: Dynamic Kibana payload mapping
- **GIVEN** a Kibana metrics document that contains a dashboard-critical metadata field and version-dependent payload fields
- **WHEN** the document is indexed
- **THEN** the template explicitly maps the dashboard metadata and dynamically maps the remaining payload
