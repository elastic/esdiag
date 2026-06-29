## ADDED Requirements

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
