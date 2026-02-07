## 1. Mapping Stats Processor

- [x] 1.1 Create `src/processor/elasticsearch/mapping_stats/data.rs` with `MappingStats`, `MappingSummary`, and optimized `Mapping` structs for permissive deserialization
- [x] 1.2 Implement `DataSource` for `MappingStats` to parse `mapping.json` using the new structs
- [x] 1.3 Implement recursive summarization logic to count field types from the `FieldDefinition` tree
- [x] 1.4 Implement `MappingStats::process` to generate the lookup map

## 2. Integration and Lookups

- [x] 2.1 Add `mapping_stats: Lookup<MappingSummary>` to `Lookups` struct in `src/processor/elasticsearch/mod.rs`
- [x] 2.2 Update `Elasticsearch::process` in `src/processor/elasticsearch/mod.rs` to initialize the `mapping_stats` lookup
- [x] 2.3 Register `MappingStats` in `src/processor/elasticsearch/collector.rs`

## 3. Indices Stats Enrichment

- [x] 3.1 Update `EnrichedIndexStatsWithSettings` in `src/processor/elasticsearch/indices_stats/processor.rs` to include a `mappings` field
- [x] 3.2 Update `EnrichedIndexStats::with_settings` in `src/processor/elasticsearch/indices_stats/processor.rs` to accept and populate the `mappings` field
- [x] 3.3 Update `IndicesStats::documents_export` in `src/processor/elasticsearch/indices_stats/processor.rs` to perform the lookup and pass it to `with_settings`

## 4. Verification

- [x] 4.1 Run `cargo clippy` to ensure code quality
- [x] 4.2 Run `cargo test` to verify the implementation
- [x] 4.3 Add a unit test for the mapping summarization logic in `src/processor/elasticsearch/mapping_stats/data.rs`

## 5. Refinements

- [x] 5.1 Update `MappingStats` structs to include `dynamic_date_formats`, `dynamic_templates`, and `_data_stream_timestamp`
- [x] 5.2 Refactor `FieldSummary` to use a single `count` map for total and type counts
- [x] 5.3 Update summarization logic to populate the new fields
- [x] 5.4 Update Elasticsearch index template `assets/elasticsearch/index_templates/metrics-index.json`
- [x] 5.5 Update unit tests for refinements

## 6. Final Refinements

- [x] 6.1 Add `date_detection`, `numeric_detection`, and `_source` (mode) to `MappingStats` structs
- [x] 6.2 Rename `FieldSummary::count` to `FieldSummary::fields`
- [x] 6.3 Update summarization logic for new fields and renamed map
- [x] 6.4 Update unit tests for final refinements
- [x] 6.5 Update Elasticsearch index template `assets/elasticsearch/index_templates/metrics-index.json` for final refinements
