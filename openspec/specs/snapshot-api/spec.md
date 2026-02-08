# Snapshot API

## Purpose
Collect and process information about Elasticsearch snapshot repositories and snapshots to assist in diagnosing backup and recovery issues.

## Requirements

### Requirement: Snapshot Repository Collection
The system SHALL collect information about all configured snapshot repositories in the Elasticsearch cluster to provide visibility into backup configurations.

#### Scenario: Successful Repository Collection
- **WHEN** the Elasticsearch diagnostic collector executes
- **THEN** it MUST perform a GET request to the `_snapshot` endpoint and capture the response.

### Requirement: Snapshot Details Collection
The system SHALL collect comprehensive details for all snapshots across all repositories to allow for auditing of backup history.

#### Scenario: Successful Snapshot Collection
- **WHEN** the Elasticsearch diagnostic collector executes
- **THEN** it MUST perform a GET request to the `_snapshot/_all/_all` endpoint and capture the response.

### Requirement: Snapshot Processor Integration
The system SHALL integrate the snapshot and repository data into the `ElasticsearchDiagnostic` processing pipeline.

#### Scenario: Data Available in Diagnostic Output
- **WHEN** the diagnostic processing for a cluster completes
- **THEN** the resulting diagnostic output MUST include the structured data retrieved from the snapshot and repository APIs.

### Requirement: Snapshot API Index Templates
The system SHALL provide Elasticsearch index templates for the snapshot repository and snapshot data streams to ensure proper indexing and mapping.

#### Scenario: Index Templates Exist
- **WHEN** the project assets are inspected
- **THEN** index templates for `settings-snapshot_repositories-esdiag` and `metadata-snapshots-esdiag` MUST exist in the `assets/` directory.
