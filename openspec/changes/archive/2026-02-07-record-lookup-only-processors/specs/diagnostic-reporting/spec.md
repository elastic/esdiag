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
