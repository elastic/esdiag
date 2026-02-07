## ADDED Requirements

### Requirement: Failure Store Support in DataStream
The `DataStream` struct SHALL include a `failure_store` field that captures the failure store configuration and indices.

#### Scenario: Parse failure store in DataStream
- **WHEN** the Elasticsearch API returns a data stream with a `failure_store` object
- **THEN** the `DataStream` struct SHALL correctly deserialize the `failure_store` field

### Requirement: Failure Store Support in DataStreamDocument
The `DataStreamDocument` struct SHALL include a `failure_store` field to persist failure store information.

#### Scenario: Convert DataStream to DataStreamDocument
- **WHEN** a `DataStream` is converted to a `DataStreamDocument`
- **THEN** the `failure_store` information SHALL be preserved in the resulting document
