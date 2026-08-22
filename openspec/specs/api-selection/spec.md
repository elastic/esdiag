# API Selection

## Purpose

Defines how the set of APIs collected and processed for a run is selected: diagnostic
types, include/exclude overrides, minimum and dependency resolution, and the
registry-derived dispatch that routes each selected source to its processor.

## Requirements

### Requirement: Diagnostic Type Selection
The system SHALL provide a `--type` CLI argument for the `collect` command to select a predefined set of APIs to collect. Valid types MUST include `minimal`, `standard`, `support`, and `comprehensive`. If not specified, the default type SHALL be `standard`. The `standard` type MUST map to the existing default set of collected APIs for each product to maintain backward compatibility.

#### Scenario: User selects minimal type
- **GIVEN** a collector orchestrator is invoked
- **WHEN** the user runs `esdiag collect --type minimal`
- **THEN** the system uses the predefined set of APIs for the minimal diagnostic type

#### Scenario: User relies on default type (Backward Compatibility)
- **GIVEN** a collector orchestrator is invoked for an Elasticsearch cluster
- **WHEN** the user runs `esdiag collect` without a `--type` argument
- **THEN** the system defaults to the predefined set of APIs for the standard diagnostic type
- **AND** the system collects exactly the same APIs as prior to this feature

### Requirement: API Inclusion Override
The system SHALL provide an `--include` CLI argument that accepts a comma-separated list of API identifiers. The system MUST add these APIs to the set of APIs selected by the diagnostic type.

#### Scenario: User includes multiple extra APIs
- **GIVEN** the standard diagnostic type is selected
- **WHEN** the user runs `esdiag collect --include nodes_hot_threads,tasks`
- **THEN** the system parses the comma-separated string
- **AND** collects all APIs from the standard type PLUS the `nodes_hot_threads` and `tasks` APIs

### Requirement: API Exclusion Override
The system SHALL provide an `--exclude` CLI argument that accepts a comma-separated list of API identifiers. The system MUST remove these APIs from the set of APIs selected by the diagnostic type, unless they are minimum required APIs or required dependencies.

#### Scenario: User excludes multiple APIs
- **GIVEN** the standard diagnostic type is selected which includes `indices_stats` and `alias`
- **WHEN** the user runs `esdiag collect --exclude indices_stats,alias`
- **THEN** the system parses the comma-separated string
- **AND** collects all APIs from the standard type EXCEPT the `indices_stats` and `alias` APIs

### Requirement: Product-Specific API Validation
The system SHALL validate all requested APIs (via type, include, or exclude arguments) against the valid API identifiers for the target product. If an invalid API identifier is requested, the system MUST fail immediately with an error before any collection operations begin.

#### Scenario: User includes an invalid Kibana API identifier
- **GIVEN** a Kibana collection run
- **WHEN** the user runs `esdiag collect --include not_a_real_kibana_api`
- **THEN** the system validates `not_a_real_kibana_api` against the allowed APIs for Kibana
- **AND** the system exits with an error before starting collection

#### Scenario: Common logic applies across products
- **GIVEN** a Logstash collection run
- **WHEN** the user runs `esdiag collect --type minimal`
- **THEN** the system validates the Logstash minimal APIs against the valid API list for Logstash
- **AND** the system proceeds with collection

### Requirement: Minimum Required APIs
The system MUST ensure that a baseline set of required APIs is always collected for the target product, regardless of the selected diagnostic type or user exclusions.

#### Scenario: User attempts to exclude a required Elasticsearch API
- **GIVEN** the `cluster` API is defined as a minimum required API for Elasticsearch
- **WHEN** the user runs `esdiag collect --exclude cluster`
- **THEN** the system ignores the exclusion for `cluster` and collects it anyway

#### Scenario: User attempts to exclude a required Kibana API
- **GIVEN** `kibana_status` and `kibana_spaces` are defined as minimum required APIs for Kibana collection
- **WHEN** the user runs `esdiag collect --exclude kibana_spaces`
- **THEN** the system ignores the exclusion for `kibana_spaces` and collects it anyway

### Requirement: API Dependency Resolution
The system MUST resolve and automatically include any dependent APIs required by the selected APIs for the target product.

#### Scenario: Selected Elasticsearch API requires another API
- **GIVEN** the `nodes_stats` API requires the `nodes` API for enrichment
- **WHEN** the user runs `esdiag collect --type minimal --include nodes_stats`
- **THEN** the system automatically includes the `nodes` API in the collection set

#### Scenario: Selected Kibana API requires spaces metadata
- **GIVEN** a Kibana source entry is marked `spaceaware: true`
- **WHEN** the user selects that API directly or indirectly through the diagnostic type
- **THEN** the system automatically includes the `kibana_spaces` API in the collection plan if it is not already present

### Requirement: Manifest API Tracking
The system SHALL record the final, resolved list of collected APIs in the Diagnostic Manifest file (`manifest.json` or similar).

#### Scenario: Recording resolved APIs
- **GIVEN** the user runs `esdiag collect --type minimal --include nodes_stats`
- **WHEN** the collector orchestrator finalizes the API list (which includes `nodes` via dependency resolution)
- **THEN** the generated Diagnostic Manifest contains an array field listing the exact API identifiers collected
- **AND** the array includes `nodes_stats`, `nodes`, and any minimum required APIs

### Requirement: Dynamic Diagnostic Type Inclusion
The system SHALL dynamically map diagnostic types to API inclusions using the keys and tags directly from the target product's embedded `sources.yml` definition, replacing product-specific hardcoded support lists where source catalogs exist. This mapping SHALL be the single mechanism for every diagnostic type: `minimal`, `standard`, `support`, and `light` all resolve from registry tags/membership, and no diagnostic-type set is maintained as a hardcoded list in code. For Elasticsearch specifically, `minimal` and `standard` SHALL derive from registry tags/membership (e.g. `tags: minimal`, `tags: standard`) rather than the hardcoded `es_base_apis` Minimal/Standard `vec!` lists, completing the migration already applied to `support` and `light`. All upstream-defined sources SHALL carry `tags: support` by default so ESDiag support bundles remain support-diagnostics compatible. For Kibana, `support`, `standard`, and `light` SHALL resolve to the full Kibana source catalog through tags until curated subsets are defined, while `minimal` SHALL resolve only the bootstrap APIs required to identify the Kibana instance and enumerate spaces.

#### Scenario: Evaluating the Kibana support diagnostic type
- **GIVEN** a user executes `esdiag collect --type support` against a Kibana host
- **WHEN** the API resolver evaluates the requested endpoints
- **THEN** it resolves all top-level API keys tagged `support` in `assets/kibana/sources.yml` to be collected

#### Scenario: Evaluating the Kibana default diagnostic type
- **GIVEN** a user executes `esdiag collect` against a Kibana host without specifying `--type`
- **WHEN** the API resolver applies the default `standard` diagnostic type
- **THEN** it resolves all top-level API keys tagged `standard` in `assets/kibana/sources.yml`

#### Scenario: Evaluating the Elasticsearch light diagnostic type
- **GIVEN** a user executes `esdiag collect --type light` against an Elasticsearch host
- **WHEN** the API resolver evaluates the requested endpoints
- **THEN** it resolves all top-level API keys that contain `tags: light` in `assets/elasticsearch/sources.yml`
- **AND** it includes the required minimum APIs for Elasticsearch

#### Scenario: Elasticsearch minimal and standard derive from tags
- **GIVEN** a user executes `esdiag collect --type minimal` or `--type standard` against an Elasticsearch host
- **WHEN** the API resolver evaluates the requested endpoints
- **THEN** it resolves the top-level API keys tagged for that type in `assets/elasticsearch/sources.yml`
- **AND** it does not consult any hardcoded `es_base_apis` Minimal/Standard list

### Requirement: Dynamic Subsystem Validation
The system SHALL validate requested `--include` and `--exclude` flags against the dynamically loaded keys of the `sources.yml` mapping, removing the need for a compile-time `ElasticsearchApi` enum. The `ElasticsearchApi` enum (and its Kibana/Logstash siblings) SHALL NOT be a second hand-maintained list of sources: it is removed, or if retained for ergonomics it MUST be generated from — or validated at startup against — the registry, never authored in parallel.

#### Scenario: User provides a valid custom include
- **GIVEN** a user executes `esdiag collect --include missing_api` where `missing_api` is defined in `sources.yml`
- **WHEN** the `ApiResolver` evaluates the inclusion list
- **THEN** it accepts the API name as valid and includes it in the final execution plan

#### Scenario: User provides an invalid custom include
- **GIVEN** a user executes `esdiag collect --include not_a_real_api` where the string does not exist as a key in `sources.yml`
- **WHEN** the `ApiResolver` evaluates the inclusion list
- **THEN** it rejects the API name and throws a validation error aborting the process before execution begins

#### Scenario: Retained enum is validated against the registry
- **GIVEN** an `ElasticsearchApi`-style enum is retained for ergonomics
- **WHEN** the system initializes
- **THEN** each variant MUST correspond to a registry key, and a variant with no matching key (or a registry key with no variant) MUST fail validation at startup

### Requirement: Registry-Derived Processing Dispatch
The system SHALL dispatch each processable source to its typed processor via a table iterated over the collection definition and keyed on the registry key, replacing the hand-written `should_process("key")` dispatch chain. For each processable source key the table SHALL resolve exactly one registered `DataSource`/`DocumentExporter` implementation. The system MUST NOT rely on a parallel hand-authored dispatch chain or enum match to route processing.

#### Scenario: Processing routes through the registry table
- **GIVEN** a processable source key selected for processing
- **WHEN** the processor determines how to transform it
- **THEN** it looks the key up in the registry-derived dispatch table and invokes the single registered implementation
- **AND** no hand-written `should_process` branch is consulted

#### Scenario: Adding a processable source is one registration
- **GIVEN** a developer adds a new processable source
- **WHEN** they add its `sources.yml` entry and register its typed implementation in the per-product table
- **THEN** it is collected, dispatched, and processed with no additional edits to a dispatch chain or an `ElasticsearchApi` enum

### Requirement: Logstash Support Diagnostic Type Expansion
The system SHALL resolve the Logstash `support` diagnostic type from the top-level keys defined in `assets/logstash/sources.yml` instead of a hardcoded API subset.

#### Scenario: Support type includes every configured Logstash source
- **GIVEN** `assets/logstash/sources.yml` defines the canonical Logstash sources
- **WHEN** the user runs `esdiag collect logstash --type support`
- **THEN** the resolver includes every top-level Logstash source key in the requested collection set before dependency resolution and exclusions are applied

### Requirement: Logstash Lighter Profile Stability
The system SHALL preserve bounded Logstash `minimal`, `standard`, and `light` profiles until `assets/logstash/sources.yml` provides metadata that defines lighter-weight subsets.

#### Scenario: Minimal Logstash collection stays narrow
- **GIVEN** the user runs `esdiag collect logstash --type minimal`
- **WHEN** the resolver builds the Logstash API plan
- **THEN** it includes only the required baseline Logstash node source and any required dependencies

#### Scenario: Standard and light keep the current bounded subset
- **GIVEN** the user runs `esdiag collect logstash --type standard` or `esdiag collect logstash --type light`
- **WHEN** the resolver builds the Logstash API plan
- **THEN** it includes the existing bounded Logstash subset rather than expanding to every key in `assets/logstash/sources.yml`

### Requirement: Logstash Identifier Normalization
The system SHALL accept both canonical Logstash `sources.yml` keys and legacy short Logstash identifiers for include/exclude handling, and it SHALL normalize the final execution plan to canonical source keys.

#### Scenario: User includes a legacy short Logstash identifier
- **GIVEN** the existing short identifier `node_stats` maps to the canonical source key `logstash_node_stats`
- **WHEN** the user runs `esdiag collect logstash --include node_stats`
- **THEN** the resolver accepts the request
- **AND** the final execution plan records `logstash_node_stats` as the collected API identifier

#### Scenario: User includes a canonical Logstash source key
- **GIVEN** `logstash_nodes_hot_threads_human` is defined in `assets/logstash/sources.yml`
- **WHEN** the user runs `esdiag collect logstash --include logstash_nodes_hot_threads_human`
- **THEN** the resolver accepts the request and includes that source in the final execution plan

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
