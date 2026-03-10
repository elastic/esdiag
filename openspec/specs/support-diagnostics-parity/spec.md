# Support Diagnostics Parity

## Purpose
TBD: Ensure esdiag achieves feature parity with legacy Support Diagnostics tool for log and API collection.

## Requirements

### Requirement: Raw API Data Source Support
The system SHALL be capable of fetching and storing any arbitrary API endpoint defined in `sources.yml` without requiring a strongly typed data model or parsing logic.

#### Scenario: Fetching an untyped API endpoint
- **GIVEN** a valid `sources.yml` mapping for an API named "some_new_api"
- **WHEN** the diagnostic collector evaluates its list of endpoints to fetch and discovers "some_new_api" has no corresponding strong type `DataSource` implementation
- **THEN** the system generates a generic raw data source for "some_new_api", fetches its string content, and saves it to the output directory using the dynamically resolved file path.

### Requirement: Parallel Raw API Collection
The system SHALL execute the collection of all generic raw API endpoints concurrently to prevent the diagnostic run from hanging or significantly increasing in duration.

#### Scenario: Executing a large batch of raw endpoints
- **GIVEN** a `sources.yml` file defining 80+ endpoints that lack strong type implementations
- **WHEN** the `ElasticsearchDiagnostic::process` method executes
- **THEN** all raw endpoints are fetched in parallel (e.g. using `tokio::spawn` or concurrent streams) alongside the core typed APIs, and their execution time does not block the core data processing tasks.

### Requirement: Full Logstash Support Collection
The system SHALL collect every selected Logstash endpoint defined in `assets/logstash/sources.yml` during a Logstash support diagnostic run.

#### Scenario: Support run expands to all Logstash source entries
- **GIVEN** the user runs `esdiag collect logstash --type support`
- **WHEN** the collection plan is built from `assets/logstash/sources.yml`
- **THEN** the collector schedules every selected Logstash source for download
- **AND** the resulting archive contains one output file per successfully collected Logstash source

### Requirement: Typed Logstash Endpoint Reuse
The system SHALL use dedicated typed collection handlers for Logstash sources that already have explicit implementations, and it SHALL avoid scheduling an additional raw fetch for the same canonical source.

#### Scenario: Node sources are collected through typed handlers
- **GIVEN** the canonical Logstash sources `logstash_node` and `logstash_node_stats` are selected for collection
- **WHEN** the collector partitions the execution plan
- **THEN** it routes those sources through their typed Logstash save handlers
- **AND** it does not enqueue a duplicate raw fetch for either source

### Requirement: Raw Logstash Endpoint Collection
The system SHALL fetch and store selected Logstash endpoints without typed handlers as raw files using the URL, file path, and extension defined in `assets/logstash/sources.yml`.

#### Scenario: Human hot threads output preserves text extension
- **GIVEN** `logstash_nodes_hot_threads_human` is selected for collection
- **WHEN** the collector resolves that source from `assets/logstash/sources.yml`
- **THEN** it fetches the configured request path for the target version
- **AND** it stores the response as `logstash_nodes_hot_threads_human.txt`

### Requirement: Dedicated Logstash Transport Path
The system SHALL use Logstash-specific client and receiver implementations for Logstash known-host collection instead of routing Logstash traffic through the Elasticsearch transport stack.

#### Scenario: Logstash known host creates Logstash transport types
- **GIVEN** a configured known host whose product is `logstash`
- **WHEN** the user runs `esdiag collect` or validates that host connection
- **THEN** the system constructs Logstash-specific client and receiver implementations
- **AND** Logstash root-response validation is performed against the Logstash response shape rather than the Elasticsearch response shape

### Requirement: Cross-Version Logstash Compatibility Validation
The system SHALL include ignored integration tests that exercise Logstash collection against externally managed Logstash instances for the supported compatibility matrix.

#### Scenario: Validate Logstash 6.8 support collection
- **GIVEN** an externally reachable Logstash `6.8.x` test instance is available
- **WHEN** the ignored compatibility test suite is run
- **THEN** it verifies that Logstash collection completes successfully for that instance

#### Scenario: Validate Logstash 7.17 support collection
- **GIVEN** an externally reachable Logstash `7.17.x` test instance is available
- **WHEN** the ignored compatibility test suite is run
- **THEN** it verifies that Logstash collection completes successfully for that instance

#### Scenario: Validate Logstash 8.19 support collection
- **GIVEN** an externally reachable Logstash `8.19.x` test instance is available
- **WHEN** the ignored compatibility test suite is run
- **THEN** it verifies that Logstash collection completes successfully for that instance

#### Scenario: Validate Logstash 9.x support collection
- **GIVEN** an externally reachable Logstash `9.x` test instance is available
- **WHEN** the ignored compatibility test suite is run
- **THEN** it verifies that Logstash collection completes successfully for that instance
