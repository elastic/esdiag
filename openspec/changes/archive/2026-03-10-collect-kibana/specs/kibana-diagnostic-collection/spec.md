## ADDED Requirements

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
