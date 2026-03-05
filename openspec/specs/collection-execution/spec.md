## ADDED Requirements

### Requirement: API Deduplication
The system SHALL ensure that no API identifier appears more than once in the final resolved list of APIs to collect.

#### Scenario: Explicit inclusion overlaps with dependency
- **GIVEN** a diagnostic type that already includes `nodes`
- **WHEN** the user runs `esdiag collect --include nodes_stats,nodes`
- **THEN** the system resolves the final list of APIs
- **AND** the `nodes` API is only executed once during the collection phase

### Requirement: Safety-Aware Execution Concurrency
The system SHALL classify registered APIs as either `Heavy` or `Light`. `Heavy` APIs MUST be executed strictly sequentially to protect the target cluster from excessive load. `Light` APIs MAY be executed concurrently (with a bounded concurrency limit) to improve collection speed.

#### Scenario: Executing a mix of APIs
- **GIVEN** `nodes_stats` is classified as `Heavy` and `cluster_health` is classified as `Light`
- **WHEN** the system begins the execution phase of the collection
- **THEN** the `nodes_stats` API is fetched sequentially without other APIs executing concurrently
- **AND** the `cluster_health` API can be fetched concurrently alongside other `Light` APIs (e.g., `licenses`)

### Requirement: Graceful API Retries
The system SHALL implement a graceful retry mechanism for individual API fetch failures during collection. If a fetch fails due to a transient error, the system MUST retry the fetch using an exponential backoff timer for up to 5 minutes and log a warning.

#### Scenario: API fetch encounters a timeout
- **GIVEN** the collection execution loop is attempting to fetch `indices_stats`
- **WHEN** the HTTP request to the cluster times out
- **THEN** the system logs a warning detailing the failure
- **AND** the system retries the `indices_stats` request using exponential backoff
- **AND** if the retries continue to fail for 5 minutes, the system continues to the next API in the queue rather than aborting the entire collection run

### Requirement: Exhaustive API Matching
The system MUST implement exhaustive pattern matching when mapping the generic API enum to the concrete fetch/save execution logic to prevent unhandled APIs at compile time.

#### Scenario: Developer adds a new API enum variant
- **GIVEN** a developer adds a new variant `IndicesRecovery` to the `ElasticsearchApi` enum
- **WHEN** they attempt to compile the `esdiag` CLI
- **THEN** the Rust compiler issues an error because the new variant is not handled in the exhaustive `match` statement within the collection execution loop

### Requirement: Role-Constrained Execution Targets
The collection execution workflow SHALL resolve host targets by role before executing each workflow phase. The collect phase SHALL use only hosts with the `collect` role, the send phase SHALL use only hosts with the `send` role, and the view phase SHALL use only hosts with the `view` role.

#### Scenario: Resolve targets for multi-phase workflow
- **GIVEN** host configuration includes hosts with `collect`, `send`, and `view` roles
- **WHEN** the workflow resolves targets for collection and output handling
- **THEN** collection calls are executed only against `collect` hosts
- **AND** send/output calls are executed only against `send` hosts
- **AND** view target resolution includes only `view` hosts
