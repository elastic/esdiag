## ADDED Requirements

### Requirement: Failure Store Index Identification
The system SHALL identify indices belonging to a data stream's failure store by checking the `failure_store.indices` field in the data stream response.

#### Scenario: Identify failure store index
- **WHEN** a data stream has a `failure_store` with an index named `.fs-metrics-index-esdiag-2026.02.07-000012`
- **THEN** the system SHALL recognize this index as part of the failure store

### Requirement: Failure Store Index Enrichment
The system SHALL enrich failure store indices with the same metadata as regular backing indices, including `type`, `dataset`, and `namespace`.

#### Scenario: Enrich failure store index
- **WHEN** a failure store index is identified for data stream `metrics-index-esdiag`
- **THEN** the system SHALL assign `type: metrics`, `dataset: index`, and `namespace: esdiag` to that index
