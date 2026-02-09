## Context

Current deserialization in `esdiag` uses `serde_json::from_reader` or `response.json::<T>()`, which loads the entire JSON document into memory before any processing begins. For large diagnostic files like `indices_stats.json` or `nodes_stats.json`, this causes peak memory usage to exceed the file size, leading to performance issues and potential OOMs on memory-constrained systems.

## Goals / Non-Goals

**Goals:**
- Implement streaming deserialization for `IndicesStats`, `NodeStats`, and `MappingStats`.
- Reduce peak memory usage by processing entries (indices or nodes) one-by-one.
- Efficiently handle deeply nested mapping structures by avoiding full in-memory representation of field properties during summary generation.
- Introduce reusable patterns for memory-efficient deserialization.

**Non-Goals:**
- Refactoring small or simple diagnostic types where memory is not a concern.
- Changing the core enrichment or export logic.

## Decisions

1. **Streaming Traits**:
   - Introduce `StreamingDataSource` trait that identifies a `DataSource` as capable of being processed in a streaming manner.
   - Introduce `StreamingReceive` trait for `Receiver` implementations to provide a stream of entries.

2. **Manual Stream Deserialization**:
   - Use `serde_json::StreamDeserializer` where inputs are concatenated JSON or similar.
   - For the common case of a large JSON object (e.g., `{ "indices": { "index1": { ... }, "index2": { ... } } }`), implement a custom `Visitor` that allows iterating over map entries without loading the entire map into memory.

3. **Refactoring Processors**:
   - Update `IndicesStats`, `NodeStats`, and `MappingStats` to no longer hold a `Vec` or `HashMap` of all entries.
   - Modify `DocumentExporter` or introduce a new trait that accepts a stream of entries.
   - For `MappingStats`, refactor the `summarize` logic to work with the stream of mapping entries to populate the `Lookup<MappingSummary>`.

4. **Buffer Reuse (Seeding)**:
   - Implement `serde::de::DeserializeSeed` for the entry types to allow reusing memory for strings and vectors within the entry structures.

## Risks / Trade-offs

- **[Risk]** Manual deserialization logic is more complex and harder to maintain than `#[derive(Deserialize)]`.
  - **[Mitigation]** Keep manual logic limited to the top-level loop; nested structures should still use `derive` where possible.
- **[Risk]** `serde_json::StreamDeserializer` may not directly support the nested map structure of some Elasticsearch APIs.
  - **[Mitigation]** Use a manual `Visitor` implementation for these specific types to skip the outer object and stream the inner map entries.
- **[Trade-off]** Streaming may be slightly slower than in-memory processing due to repeated trait dispatch or smaller buffers, but it is necessary for stability.
