# Mapping Summaries

## Purpose
Provide concise summaries of index mappings (field type counts, settings, and metadata) to enrich index statistics without importing full mapping files. This enables efficient diagnostic analysis and visualization of index structures.

## Requirements

### Requirement: Mapping Statistics Processing
The system SHALL provide a `MappingStats` processor capable of reading index mapping files (e.g., `mapping.json`) and extracting structural metadata. The implementation SHALL use optimized data structures to minimize memory overhead when processing large mapping files.

#### Scenario: Successfully parse mapping file
- **WHEN** a mapping file containing index definitions is processed
- **THEN** the system SHALL extract the `dynamic` setting, `dynamic_date_formats`, `dynamic_templates` count, `_data_stream_timestamp` status, and `_meta` object for each index

### Requirement: Field Type Summarization
The `MappingStats` processor SHALL calculate a summary of field types for each index, grouped under a `fields` object.

#### Scenario: Count field types in a mapping
- **WHEN** an index mapping is processed with 5 text fields and 10 keyword fields
- **THEN** the resulting summary SHALL report `total: 15`, `text: 5` and `keyword: 10` under the `fields` object

### Requirement: Index Stats Enrichment
The `IndicesStats` exporter SHALL enrich its output with the mapping summary obtained from `MappingStats`.

#### Scenario: Enrich index stats with mapping summary
- **WHEN** index statistics are being exported for an index named "logs-001"
- **AND** a mapping summary exists for "logs-001"
- **THEN** the exported document SHALL include a `mappings` object containing the summarized field counts, multi-field counts, `dynamic` setting, and `_meta` information

### Requirement: Multi-Field Counting
The `MappingStats` processor SHALL count the number of fields that have one or more multi-fields defined.

#### Scenario: Count multi-fields in a mapping
- **WHEN** an index mapping is processed with 3 fields, one of which has a `fields` property
- **THEN** the resulting summary SHALL report `total: 1` under the `multi-fields` object
