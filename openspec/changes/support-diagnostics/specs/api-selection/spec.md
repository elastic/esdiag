## MODIFIED Requirements

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