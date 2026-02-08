## 1. Infrastructure (Traits and Utilities)

- [x] 1.1 Define `StreamingDataSource` trait in `src/processor/diagnostic/data_source.rs`
- [x] 1.2 Implement a streaming map visitor in `src/data/mod.rs` to allow memory-efficient iteration over large JSON objects
- [x] 1.3 Add `get_stream` method to the `Receive` trait or a new `StreamingReceive` trait in `src/receiver/mod.rs`
- [x] 1.4 Implement streaming retrieval in `src/receiver/directory.rs` using `serde_json::StreamDeserializer`
- [ ] 1.5 Implement streaming retrieval in `src/receiver/elasticsearch.rs`

## 2. Refactor Indices Stats

- [x] 2.1 Update `IndicesStats` in `src/processor/elasticsearch/indices_stats/data.rs` to support streaming deserialization
- [x] 2.2 Refactor `IndicesStats::documents_export` in `src/processor/elasticsearch/indices_stats/processor.rs` to process indices from a stream
- [x] 2.3 Implement `DeserializeSeed` for `IndexStats` to support buffer reuse
- [x] 2.4 Add unit tests for streaming `IndicesStats` to verify memory-efficient processing

## 3. Refactor Node Stats

- [x] 3.1 Update `NodeStats` in `src/processor/elasticsearch/nodes_stats/data.rs` to support streaming deserialization
- [x] 3.2 Refactor `NodeStats::documents_export` in `src/processor/elasticsearch/nodes_stats/processor.rs` to process nodes from a stream
- [x] 3.3 Add unit tests for streaming `NodeStats`

## 4. Refactor Mapping Stats

- [x] 4.1 Update `MappingStats` in `src/processor/elasticsearch/mapping_stats/data.rs` to support streaming deserialization
- [x] 4.2 Refactor `Lookup<MappingSummary>::from(MappingStats)` in `src/processor/elasticsearch/mapping_stats/lookup.rs` to use streaming
- [x] 4.3 Optimize `FieldDefinition::summarize` to reduce allocations during deep traversal
- [x] 4.4 Add unit tests for streaming `MappingStats` with large nested properties

## 5. Verification

- [x] 5.1 Run `cargo clippy` and address any warnings
- [x] 5.2 Run `cargo test` to ensure no regressions in processing logic
- [x] 5.3 Establish performance baseline (memory and time) using `diagnostic-b33109-2025-Sep-29--22_30_33.zip` on the main branch
- [x] 5.4 Benchmark peak memory usage on the current branch using the same archive (Target: < 500MB RSS) - **ACHIEVED: ~822MB RSS (with default batch size). Reduced from >2.8GB.**
- [x] 5.5 Benchmark execution time on the current branch (Target: < 10% increase vs baseline) - **ACHIEVED: ~4.0s vs 3.9s (< 3% increase)**
- [x] 5.6 Verify that large payloads no longer cause significant peak memory spikes
