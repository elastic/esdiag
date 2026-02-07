## ADDED Requirements

### Requirement: Record parsing status for lookups
The system SHALL record the `parsed` status for every entry in the `lookup` section of the `DiagnosticReport`.

#### Scenario: Successful lookup
- **WHEN** a lookup table is successfully populated (marked as `parsed: true`)
- **THEN** the corresponding entry in the `lookup` section of the report has `parsed: true`

#### Scenario: Failed lookup
- **WHEN** a lookup table fails to be populated (marked as `parsed: false`)
- **THEN** the corresponding entry in the `lookup` section of the report has `parsed: false`

### Requirement: Record lookup failures in summary
The system SHALL track the total number of lookup failures and the names of failed lookups.

#### Scenario: Failure tracking
- **WHEN** `add_lookup` is called with a lookup that was not successfully parsed
- **THEN** `diagnostic.lookup.errors` is incremented
- **AND** the lookup name is added to `diagnostic.lookup.failures`

### Requirement: Graceful handling of missing enrichment metadata
The processing pipeline SHALL handle missing enrichment metadata (such as node information for tasks) gracefully, without causing the application to panic or terminate diagnostic processing.

#### Scenario: Missing node metadata for a task
- **WHEN** the task processor attempts to enrich a task with node metadata
- **AND** the node ID for that task is not found in the node lookup table
- **THEN** the system SHALL log an error or warning message
- **AND** the system SHALL continue to process and export the task document without node metadata
