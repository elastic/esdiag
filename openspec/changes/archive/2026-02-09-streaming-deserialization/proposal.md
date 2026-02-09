## Why

Large JSON diagnostics payloads (e.g., `node_stats`, `indices_stats`) currently flood memory because they are deserialized all at once using generic `#[derive(Deserialize)]`. This causes high peak memory usage during input processing, which can lead to OOM errors on memory-constrained systems when processing large diagnostics.

## What Changes

Implement streaming deserialization and buffer reuse for large data structures to reduce peak memory overhead during input processing.

- **Streaming**: Use `serde_json::StreamDeserializer` to process entries in batches instead of loading the entire top-level object into memory.
- **Seeding**: Use `serde::de::DeserializeSeed` to reuse buffers (e.g., `Vec<T>`) across entries, reducing allocations.
- **Targeting**: Initially target `node_stats`, `indices_stats`, and `mapping_stats` for Elasticsearch processors.

## Capabilities

### New Capabilities
- `streaming-deserialization`: Provides reusable traits and utilities for processing large diagnostics payloads in a memory-efficient, streaming manner.

### Modified Capabilities
- (None): No existing high-level requirement specifications are changing; this is a performance and memory-efficiency optimization of the underlying processing logic.

## Impact

- `src/processor/elasticsearch/indices_stats/`: Deserialization logic will be refactored to use streaming.
- `src/processor/elasticsearch/nodes_stats/`: Deserialization logic will be refactored to use streaming.
- `src/processor/elasticsearch/mapping_stats/`: Deserialization logic will be refactored to use streaming. This is particularly critical as mappings can have tens of thousands of entries with deeply nested parent/child structures.
- `serde` and `serde_json` usage will become more complex (manual `Visitor` or `DeserializeSeed` implementations).
