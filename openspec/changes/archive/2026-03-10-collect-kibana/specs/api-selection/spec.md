## MODIFIED Requirements

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

### Requirement: Dynamic Diagnostic Type Inclusion
The system SHALL dynamically map diagnostic types to API inclusions using the keys and tags directly from the target product's embedded `sources.yml` definition, replacing product-specific hardcoded support lists where source catalogs exist. For Kibana, `support`, `standard`, and `light` SHALL resolve to the full Kibana source catalog until curated subsets are defined, while `minimal` SHALL resolve only the bootstrap APIs required to identify the Kibana instance and enumerate spaces.

#### Scenario: Evaluating the Kibana support diagnostic type
- **GIVEN** a user executes `esdiag collect --type support` against a Kibana host
- **WHEN** the API resolver evaluates the requested endpoints
- **THEN** it resolves all top-level API keys present in `assets/kibana/sources.yml` to be collected

#### Scenario: Evaluating the Kibana default diagnostic type
- **GIVEN** a user executes `esdiag collect` against a Kibana host without specifying `--type`
- **WHEN** the API resolver applies the default `standard` diagnostic type
- **THEN** it resolves the same full Kibana source catalog used by the Kibana `support` type

#### Scenario: Evaluating the Elasticsearch light diagnostic type
- **GIVEN** a user executes `esdiag collect --type light` against an Elasticsearch host
- **WHEN** the API resolver evaluates the requested endpoints
- **THEN** it resolves all top-level API keys that contain `tags: light` in `assets/elasticsearch/sources.yml`
- **AND** it includes the required minimum APIs for Elasticsearch
