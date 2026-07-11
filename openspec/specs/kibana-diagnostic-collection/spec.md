# Kibana Diagnostic Collection

## Purpose
Define how `esdiag collect` executes Kibana API collection from the Kibana source catalog, including space-aware expansion, pagination, output layout, and compatibility validation.

## Requirements

### Requirement: Kibana support collection
The system SHALL allow `esdiag collect` to execute against a Kibana target and collect the Kibana APIs selected for the diagnostic run using `assets/kibana/sources.yml` as the source of truth for endpoint discovery.

#### Scenario: Running a support collection against Kibana
- **WHEN** a user runs `esdiag collect` against a Kibana host with a diagnostic selection that includes the full support set
- **THEN** the system resolves Kibana API identifiers from `assets/kibana/sources.yml`
- **AND** the system fetches each resolved endpoint from the Kibana host instead of failing with an unimplemented-product error

### Requirement: Space-aware endpoint expansion
The system SHALL expand Kibana source entries marked `spaceaware: true` across every accessible Kibana space and persist each space-scoped response separately so no space overwrites another.

#### Scenario: Collecting a space-aware Kibana API
- **WHEN** the system collects a source entry whose resolved configuration has `spaceaware: true`
- **THEN** the system discovers the accessible Kibana spaces before issuing that request
- **AND** the system executes the request once per discovered space
- **AND** the system stores each space-scoped response using a unique output path

### Requirement: Paginated endpoint traversal
The system SHALL continue requesting Kibana source entries marked with a `paginate` field until every page of results has been collected.

#### Scenario: Collecting a paginated Kibana API
- **WHEN** the system collects a source entry whose resolved configuration includes `paginate: per_page`
- **THEN** the system appends the configured pagination parameter to successive requests as needed to retrieve every page
- **AND** the system persists the complete paginated result set without truncating to the first response page

### Requirement: Source-defined Kibana output layout
The system SHALL write Kibana collection outputs using the filename, extension, and subdirectory information resolved from the matching Kibana source entry, while adding scope-specific path segments when required to keep per-space or per-page artifacts distinct.

#### Scenario: Writing a scoped Kibana artifact
- **WHEN** a Kibana source entry resolves to a configured output path and the request is scoped by space or pagination
- **THEN** the system preserves the configured file naming from `assets/kibana/sources.yml`
- **AND** the system augments the output path so repeated requests for the same API do not overwrite one another

### Requirement: Kibana version-matrix validation
The system SHALL include ignored tests that validate Kibana collection behavior against externally managed Kibana instances on `6.8.x`, `7.17.x`, `8.19.x`, and `9.x`.

#### Scenario: Running ignored external Kibana compatibility tests
- **WHEN** a maintainer runs the ignored Kibana integration test suite with access to the required external services
- **THEN** the suite executes compatibility coverage against Kibana `6.8.x`, `7.17.x`, `8.19.x`, and `9.x`
- **AND** the tests may be skipped in normal local and CI runs when the external services are unavailable

### Requirement: Shared Kibana client integration
The system SHALL execute Kibana diagnostic collection HTTP requests through the `kibana-sync` Kibana client while preserving the existing ESDiag Kibana collection contract for authentication, version discovery, space discovery, raw response capture, retry classification, manifest metadata, and archive output layout.

#### Scenario: Kibana host authentication is mapped into the shared client
- **GIVEN** a saved Kibana host configured with Basic authentication, API key authentication, or no authentication
- **WHEN** ESDiag creates the Kibana receiver for that host
- **THEN** the receiver builds a `kibana-sync` Kibana client with the equivalent authentication mode
- **AND** Kibana requests continue to include the required `kbn-xsrf` behavior supplied by the shared client

#### Scenario: Version discovery preserves diagnostic metadata
- **GIVEN** a Kibana collection run
- **WHEN** the receiver resolves the Kibana version from `/api/status`
- **THEN** the request is executed through the `kibana-sync` client
- **AND** the diagnostic manifest records the same Kibana version value expected by existing Kibana collection behavior

#### Scenario: Raw response metrics are preserved
- **GIVEN** a Kibana source endpoint returns a successful response
- **WHEN** ESDiag collects that endpoint through the `kibana-sync` client
- **THEN** the resulting raw response records the HTTP status, response time in milliseconds, response size in bytes, and response body
- **AND** the collected artifact is written using the same source-defined archive path rules as before

#### Scenario: Existing collection concurrency is preserved
- **GIVEN** a Kibana collection run executes multiple source endpoints
- **WHEN** ESDiag builds the `kibana-sync` client for the run
- **THEN** the shared client is configured with ESDiag's existing Kibana request concurrency limit
- **AND** the migration does not increase effective parallel request pressure on Kibana

#### Scenario: Non-success responses preserve retry decisions
- **GIVEN** a Kibana source endpoint returns HTTP 408, HTTP 429, or a 5xx status
- **WHEN** ESDiag handles the response from the `kibana-sync` client
- **THEN** the failure is represented with the HTTP status and body available to ESDiag's Kibana retry policy
- **AND** the collector applies the same retry behavior used before the shared client migration

#### Scenario: Space-aware collection is not double-prefixed
- **GIVEN** a Kibana source entry is marked `spaceaware: true`
- **WHEN** ESDiag expands that source across discovered spaces
- **THEN** each request is sent to exactly one intended Kibana space path
- **AND** no request path contains duplicate space prefixes introduced by both ESDiag and the shared client

#### Scenario: Multipart Kibana requests remain supported
- **GIVEN** an ESDiag Kibana workflow sends an NDJSON saved-object payload as multipart form data
- **WHEN** the request is executed through the `kibana-sync` client
- **THEN** the payload is sent using Kibana-compatible multipart upload semantics
- **AND** callers do not need to reimplement multipart request construction in ESDiag

### Requirement: Bundled Kibana asset setup
The system SHALL store bundled Kibana setup assets in the `kibana-sync` filesystem bundle layout and SHALL embed those assets into release binaries as a single generated bundle artifact.

#### Scenario: Kibana assets use the bundle directory layout
- **GIVEN** ESDiag's bundled Kibana assets
- **WHEN** the assets are read with `kibana-sync` filesystem bundle support
- **THEN** the bundle contains the `esdiag` space definition and saved-object manifest
- **AND** all manifest-listed saved objects are available as per-object JSON resources under the `esdiag` space
- **AND** empty agents, tools, and workflows manifests are present for future supported asset families

#### Scenario: Release binaries embed the generated Kibana bundle
- **GIVEN** ESDiag is built with bundled assets
- **WHEN** the build script prepares Kibana assets
- **THEN** it creates one generated Kibana asset bundle from `assets/kibana`
- **AND** the generic embedded assets tree excludes raw `kibana/**` files
- **AND** setup reads Kibana manifests and object files from the generated bundle

#### Scenario: Kibana setup imports bundled assets
- **GIVEN** Elasticsearch and Kibana are reachable by `esdiag setup`
- **WHEN** setup runs for Kibana assets
- **THEN** ESDiag creates the `esdiag` Kibana space using the bundled space definition
- **AND** imports all bundled saved objects into that space through Kibana's saved-object import API
