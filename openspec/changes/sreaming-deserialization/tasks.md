## 1. Infrastructure (Traits and Utilities)

- [ ] 1.1 Define `StreamingDataSource` trait in `src/processor/diagnostic/data_source.rs`
- [ ] 1.2 Implement a streaming map visitor in `src/data/mod.rs` to allow memory-efficient iteration over large JSON objects
- [ ] 1.3 Add `get_stream` method to the `Receive` trait or a new `StreamingReceive` trait in `src/receiver/mod.rs`
- [ ] 1.4 Implement streaming retrieval in `src/receiver/directory.rs` using `serde_json::StreamDeserializer`
- [ ] 1.5 Implement streaming retrieval in `src/receiver/elasticsearch.rs`

## 2. Refactor Indices Stats

- [ ] 2.1 Update `IndicesStats` in `src/processor/elasticsearch/indices_stats/data.rs` to support streaming deserialization
- [ ] 2.2 Refactor `IndicesStats::documents_export` in `src/processor/elasticsearch/indices_stats/processor.rs` to process indices from a stream
- [ ] 2.3 Implement `DeserializeSeed` for `IndexStats` to support buffer reuse
- [ ] 2.4 Add unit tests for streaming `IndicesStats` to verify memory-efficient processing

## 3. Refactor Node Stats

- [ ] 3.1 Update `NodeStats` in `src/processor/elasticsearch/nodes_stats/data.rs` to support streaming deserialization
- [ ] 3.2 Refactor `NodeStats::documents_export` in `src/processor/elasticsearch/nodes_stats/processor.rs` to process nodes from a stream
- [ ] 3.3 Add unit tests for streaming `NodeStats`

## 4. Refactor Mapping Stats

- [ ] 4.1 Update `MappingStats` in `src/processor/elasticsearch/mapping_stats/data.rs` to support streaming deserialization
- [ ] 4.2 Refactor `Lookup<MappingSummary>::from(MappingStats)` in `src/processor/elasticsearch/mapping_stats/lookup.rs` to use streaming
- [ ] 4.3 Optimize `FieldDefinition::summarize` to reduce allocations during deep traversal
- [ ] 4.4 Add unit tests for streaming `MappingStats` with large nested properties

## 5. Verification

- [ ] 4.1 Run `cargo clippy` and address any warnings
- [ ] 4.2 Run `cargo test` to ensure no regressions in processing logic
- [ ] 4.3 Verify that large payloads no longer cause significant peak memory spikes
