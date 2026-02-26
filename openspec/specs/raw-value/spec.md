# Capability: raw-value

## Purpose
TBD: Implement raw-value capability for efficient JSON parsing without building full DOM.

## Requirements

### Requirement: RawValue Parsing Strategy
The application MUST parse large, flexible, unknown JSON payloads in diagnostics without constructing a DOM representation in memory, relying instead on raw byte serialization to avoid heap fragmentation and CPU overhead.

#### Scenario: Streaming memory consumption
- **GIVEN** a JSON diagnostic file containing large nested JSON structures (e.g. `nodes_stats`)
- **WHEN** the file is parsed by the streaming `serde` implementation
- **THEN** the objects MUST use a continuous heap allocation like `Box<RawValue>` to eliminate the thousands of fragmented inner nodes of `serde_json::Value`.

#### Scenario: Object Serialization
- **GIVEN** a `Box<RawValue>` parsed from an incoming diagnostic file
- **WHEN** the exporter serializes the object back into NDJSON for Elasticsearch ingestion
- **THEN** the raw byte string MUST be written to the output stream transparently and exactly as it was received.
