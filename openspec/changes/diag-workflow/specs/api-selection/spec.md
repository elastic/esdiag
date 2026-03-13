## ADDED Requirements

### Requirement: Processing Profile Selection
The web `Process` panel SHALL expose a diagnostic product selector and a diagnostic type selector for the current processing workflow. The available advanced processing options SHALL update based on the selected product and diagnostic type.

#### Scenario: User changes the processing profile
- **GIVEN** the `Process` panel is enabled
- **WHEN** the user selects a diagnostic product and diagnostic type
- **THEN** the workflow resolves the processing option set for that product/type combination
- **AND** the advanced options surface updates to reflect that resolved set

### Requirement: Implemented Option Filtering
The advanced processing options surface SHALL list only API options that are fully implemented for the selected product and diagnostic type. A processing option is fully implemented when it has a concrete processor implementation for that product in the application processor set. If the runtime implementation cannot infer that set directly, the system SHALL use an equivalent per-product authoritative enum or registry. That authoritative enum or registry SHALL be allowed to include dependency metadata used for required processor locking. Options that exist in source definitions but are not fully implemented SHALL NOT be displayed as selectable overrides.

#### Scenario: Selected diagnostic type includes partial implementations
- **GIVEN** a diagnostic product/type mapping includes both fully implemented and not-yet-implemented API options
- **WHEN** the user expands the advanced options accordion
- **THEN** only the fully implemented API options are shown as checkboxes
- **AND** the workflow excludes unsupported options from user selection

#### Scenario: Product uses authoritative implemented-option registry
- **GIVEN** the runtime processing workflow cannot directly infer the implemented option list from product processor modules
- **WHEN** the system resolves advanced processing options for a product
- **THEN** it uses the product's authoritative enum or registry of implemented processors
- **AND** only options in that authoritative set are shown as selectable overrides

#### Scenario: Registry also carries dependency metadata
- **GIVEN** a product uses an authoritative enum or registry for advanced processing options
- **WHEN** the workflow resolves both selectable and required processing options
- **THEN** the same authoritative enum or registry may provide dependency metadata for option locking
- **AND** the workflow uses that metadata to keep required dependent processors included

### Requirement: Advanced Processing Overrides
When advanced processing options are visible, the user SHALL be able to override the default processing subset by selecting a checkbox list of the fully implemented API options resolved for the chosen product and diagnostic type.

#### Scenario: User narrows processing to a supported subset
- **GIVEN** the `Process` panel is enabled for a diagnostic whose default type includes more APIs than the user wants to process
- **WHEN** the user selects a subset of the available advanced option checkboxes
- **THEN** the workflow processes only the selected implemented API options for that product/type selection

### Requirement: Required Processing Option Locking
The advanced processing options surface SHALL prevent users from opting out of processors that are required by minimum processing rules, direct processor dependencies, or metadata/manifest construction. Required processors SHALL remain included in the resolved processing plan even when the user narrows the selectable advanced list.

#### Scenario: Dependency-required processor remains enabled
- **GIVEN** an Elasticsearch processing selection includes `node_stats`
- **WHEN** the workflow resolves advanced processing options
- **THEN** `node_settings` is marked as required and remains included in the processing plan
- **AND** the UI does not allow the user to deselect `node_settings` while `node_stats` is selected

#### Scenario: Metadata and manifest processors remain enabled
- **GIVEN** the user is customizing Elasticsearch processing options
- **WHEN** the workflow resolves required processing rules
- **THEN** `version` and `cluster_settings_defaults` remain included as required processors
- **AND** any processor needed to build diagnostic metadata or manifest output remains locked on even if it would otherwise appear in the advanced list
