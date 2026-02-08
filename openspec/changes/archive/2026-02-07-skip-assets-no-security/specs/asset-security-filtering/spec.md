## ADDED Requirements

### Requirement: Skip security-dependent assets when security is disabled
The system SHALL identify if an asset depends on security being enabled and skip its collection if the target Elasticsearch cluster does not have security enabled.

#### Scenario: Asset requires security and security is enabled
- **WHEN** the system processes an asset marked as security-dependent
- **AND** the target Elasticsearch cluster has security enabled
- **THEN** the system SHALL proceed with collecting the asset

#### Scenario: Asset requires security and security is disabled
- **WHEN** the system processes an asset marked as security-dependent
- **AND** the target Elasticsearch cluster does NOT have security enabled
- **THEN** the system SHALL skip the collection of that asset without logging an error

### Requirement: Detect security status of Elasticsearch cluster
The system SHALL determine the security status of the target Elasticsearch cluster during the setup phase.

#### Scenario: Security status detection
- **WHEN** the system connects to the Elasticsearch cluster
- **THEN** it SHALL verify if security is enabled (e.g., by checking `_xpack/usage`)

#### Scenario: Security detection robustness
- **WHEN** the security detection API returns 401 or 403
- **THEN** the system SHALL assume security is enabled
- **WHEN** the security detection API returns 404
- **THEN** the system SHALL assume security is disabled
- **WHEN** the security detection API returns any other error
- **THEN** the system SHALL fail with a descriptive error message
