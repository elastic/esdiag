## Why

Bringing in full mapping files to Elasticsearch is inefficient and often unnecessary. However, a summary of field types per index is extremely useful for visualizations and diagnostic analysis, providing a clear breakdown of index structure without the overhead of the full mapping. To ensure efficiency even with large mapping files (thousands of fields), the implementation will use optimized Rust structs for deserialization instead of generic JSON values.

## What Changes

- **New Mapping Processor**: Create a `MappingStats` processor that parses `mapping.json` files from the diagnostic.
- **Mapping Summarization**: Extract field type counts, `dynamic` setting, `dynamic_date_formats`, `dynamic_templates` count, `_data_stream_timestamp` status, and `_meta` information from mappings.
- **Indices Stats Enrichment**: Update the `IndicesStats` exporter to perform a lookup against the collected `MappingStats` and include a summarized `mappings` object in the exported documents.
- **Data Stream Update**: The `mappings` object will be persisted in the `metrics-index-esdiag` data stream.

## Capabilities

### New Capabilities
- `mapping-summaries`: Defines the logic for parsing `mapping.json`, calculating field type distributions, and enriching index statistics with these summaries.

### Modified Capabilities
- None: Existing index stats processing remains functionally the same, but will now include enriched data if available.

## Impact

- **Affected Code**: 
  - `src/processor/elasticsearch/indices_stats/`: Needs enrichment logic.
  - New module `src/processor/elasticsearch/mapping_stats/`: To handle mapping parsing.
- **APIs**: No changes to external APIs.
- **Dependencies**: No new dependencies.
