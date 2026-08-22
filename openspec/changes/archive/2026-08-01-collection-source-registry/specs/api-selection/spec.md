## MODIFIED Requirements

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

## ADDED Requirements

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
