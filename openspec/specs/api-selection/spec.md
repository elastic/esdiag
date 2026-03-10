## ADDED Requirements

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
The system SHALL validate all requested APIs (via type, include, or exclude arguments) against a list of valid API identifiers for the target product. If an invalid API identifier is requested, the system MUST fail immediately with an error before any collection operations begin.

#### Scenario: User includes an invalid API identifier
- **GIVEN** an Elasticsearch collection run
- **WHEN** the user runs `esdiag collect --include invalid_api_name`
- **THEN** the system validates `invalid_api_name` against the allowed APIs for Elasticsearch
- **AND** the system exits with an error before starting collection

#### Scenario: Common logic applies across products
- **GIVEN** a Logstash collection run
- **WHEN** the user runs `esdiag collect --type minimal`
- **THEN** the system validates the Logstash minimal APIs against the valid API list for Logstash
- **AND** the system proceeds with collection

### Requirement: Minimum Required APIs
The system MUST ensure that a baseline set of required APIs (e.g., `cluster`, `diagnostic_manifest` for Elasticsearch) are always collected, regardless of the selected diagnostic type or user exclusions.

#### Scenario: User attempts to exclude a required API
- **GIVEN** the `cluster` API is defined as a minimum required API
- **WHEN** the user runs `esdiag collect --exclude cluster`
- **THEN** the system ignores the exclusion for `cluster` and collects it anyway

### Requirement: API Dependency Resolution
The system MUST resolve and automatically include any dependent APIs required by the selected APIs.

#### Scenario: Selected API requires another API
- **GIVEN** the `nodes_stats` API requires the `nodes` API for enrichment
- **WHEN** the user runs `esdiag collect --type minimal --include nodes_stats` (where minimal does not include `nodes`)
- **THEN** the system automatically includes the `nodes` API in the collection set

### Requirement: Manifest API Tracking
The system SHALL record the final, resolved list of collected APIs in the Diagnostic Manifest file (`manifest.json` or similar).

#### Scenario: Recording resolved APIs
- **GIVEN** the user runs `esdiag collect --type minimal --include nodes_stats`
- **WHEN** the collector orchestrator finalizes the API list (which includes `nodes` via dependency resolution)
- **THEN** the generated Diagnostic Manifest contains an array field listing the exact API identifiers collected
- **AND** the array includes `nodes_stats`, `nodes`, and any minimum required APIs

### Requirement: Dynamic Diagnostic Type Inclusion
The system SHALL dynamically map diagnostic types to API inclusions using the keys and tags directly from the embedded `sources.yml` definition, replacing hardcoded subsets.

#### Scenario: Evaluating the "support" diagnostic type
- **GIVEN** a user executes `esdiag collect --type support`
- **WHEN** the `ApiResolver` evaluates the requested endpoints
- **THEN** it resolves all top-level API keys present in `assets/elasticsearch/sources.yml` to be collected

#### Scenario: Evaluating the "light" diagnostic type
- **GIVEN** a user executes `esdiag collect --type light`
- **WHEN** the `ApiResolver` evaluates the requested endpoints
- **THEN** it resolves all top-level API keys that contain `tags: light` in `assets/elasticsearch/sources.yml` (plus required minimums like `cluster` and `nodes`) to be collected

### Requirement: Dynamic Subsystem Validation
The system SHALL validate requested `--include` and `--exclude` flags against the dynamically loaded keys of the `sources.yml` mapping, removing the need for a compile-time `ElasticsearchApi` enum.

#### Scenario: User provides a valid custom include
- **GIVEN** a user executes `esdiag collect --include missing_api` where `missing_api` is defined in `sources.yml`
- **WHEN** the `ApiResolver` evaluates the inclusion list
- **THEN** it accepts the API name as valid and includes it in the final execution plan

#### Scenario: User provides an invalid custom include
- **GIVEN** a user executes `esdiag collect --include not_a_real_api` where the string does not exist as a key in `sources.yml`
- **WHEN** the `ApiResolver` evaluates the inclusion list
- **THEN** it rejects the API name and throws a validation error aborting the process before execution begins

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
