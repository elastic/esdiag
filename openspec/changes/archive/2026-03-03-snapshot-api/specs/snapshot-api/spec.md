## ADDED Requirements

### Requirement: Snapshot Repository Collection
The system SHALL collect information about all configured snapshot repositories in the Elasticsearch cluster.

#### Scenario: Successful Repository Collection
- **WHEN** the Elasticsearch diagnostic collector executes
- **THEN** it MUST perform a GET request to the `_snapshot` endpoint and capture repository records for diagnostic output.

### Requirement: Snapshot Details Collection
The system SHALL collect details for snapshots across repositories from Elasticsearch.

#### Scenario: Successful Snapshot Collection
- **WHEN** the Elasticsearch diagnostic collector executes
- **THEN** it MUST perform a GET request to the `/_snapshot/*/*?verbose=false` endpoint and process snapshot entries for diagnostic output.

### Requirement: Streaming Snapshot Deserialization
Snapshot detail collection SHALL use streaming deserialization semantics equivalent to the `indices_stats` processor pattern.

#### Scenario: Snapshot Items Are Emitted Incrementally
- **GIVEN** a large `/_snapshot/*/*?verbose=false` payload
- **WHEN** the payload is deserialized
- **THEN** snapshot entries MUST be emitted incrementally via a streaming datasource interface, rather than requiring full payload materialization before processing.

### Requirement: Streaming Snapshot Export
Snapshot document export SHALL use channel-based streaming export semantics equivalent to the `indices_stats` processor pattern.

#### Scenario: Snapshot Documents Are Exported Through Document Channels
- **WHEN** snapshot entries are processed
- **THEN** documents MUST be sent through exporter document channels with bounded buffering and batched sends.
- **AND** processor completion MUST wait for channel consumers to finish and merge their summaries.

### Requirement: Structured Snapshot Documents
The system SHALL output structured snapshot and snapshot-repository documents with stable core fields.

#### Scenario: Core Snapshot Fields Are Present
- **WHEN** a snapshot document is exported
- **THEN** it MUST include stable core fields for snapshot identity and status, including snapshot name, repository, state, snapshot contents (`indices` and data streams when available), and timing fields when available.

### Requirement: Resilient Stream Error Handling
The snapshot streaming path SHALL continue processing when an individual streamed entry fails to parse or export.

#### Scenario: Per-Entry Failure During Stream
- **GIVEN** a failure for one streamed snapshot entry
- **WHEN** the stream is processed
- **THEN** the system MUST log the failure, continue processing remaining entries, and finalize with a processor summary.

### Requirement: Snapshot Processor Integration
The system SHALL integrate snapshot and repository data collection into the `ElasticsearchDiagnostic` processing pipeline.

#### Scenario: Data Available in Diagnostic Output
- **WHEN** diagnostic processing for a cluster completes
- **THEN** output MUST include structured repository and snapshot data from snapshot APIs.

### Requirement: Snapshot API Index Templates
The system SHALL provide Elasticsearch index templates for snapshot repository and snapshot data streams.

#### Scenario: Index Templates Exist
- **WHEN** project assets are inspected
- **THEN** templates for `settings-repository-esdiag` and `logs-snapshot-esdiag` MUST exist in `assets/`.

### Requirement: Snapshot Date Extraction
The system SHALL derive a `snapshot.date` field from snapshot names when a date token is present.

#### Scenario: Snapshot Name Contains Date
- **GIVEN** a snapshot name containing a `YYYY.MM.DD` token
- **WHEN** the snapshot document is built
- **THEN** `snapshot.date` MUST be populated from that token as a date field.

#### Scenario: Snapshot Name Does Not Contain Date
- **GIVEN** a snapshot name without a `YYYY.MM.DD` token
- **WHEN** the snapshot document is built
- **THEN** `snapshot.date` MUST be absent or null, and document export MUST continue.
