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
