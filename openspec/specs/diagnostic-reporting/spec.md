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

### Requirement: Viewer-Aware Kibana Link Selection
The system SHALL determine the Kibana base URL for final processed-diagnostic reporting by first resolving the explicit output target's saved viewer host, and SHALL fall back to `ESDIAG_KIBANA_URL` when no saved viewer host is available. If `ESDIAG_KIBANA_SPACE` is present, the system SHALL append the configured space path to the selected Kibana base URL before constructing the final Kibana link.

#### Scenario: Saved viewer host overrides environment Kibana URL
- **GIVEN** a processed diagnostic is sent to a saved Elasticsearch host with role `send`
- **AND** that saved host references a saved Kibana viewer host
- **AND** `ESDIAG_KIBANA_URL` is also set
- **WHEN** final processing reporting builds the Kibana link
- **THEN** the link uses the saved viewer host URL as its base URL

#### Scenario: Environment fallback is used when no saved viewer host exists
- **GIVEN** a processed diagnostic completes without a resolved saved viewer host
- **AND** `ESDIAG_KIBANA_URL` is set
- **WHEN** final processing reporting builds the Kibana link
- **THEN** the link uses `ESDIAG_KIBANA_URL` as its base URL

#### Scenario: Default Kibana URL is used when no override source is available
- **GIVEN** a processed diagnostic completes without a resolved saved viewer host
- **AND** `ESDIAG_KIBANA_URL` is not explicitly set
- **WHEN** final processing reporting completes
- **THEN** the link uses the default Kibana base URL
