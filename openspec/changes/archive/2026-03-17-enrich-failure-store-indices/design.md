## Context

Elasticsearch introduced a failure store feature for data streams. The indices associated with this failure store are returned in a `failure_store.indices` field, which is distinct from the regular `indices` field. `esdiag` currently only processes the `indices` field, meaning failure store indices miss out on metadata enrichment (type, dataset, namespace).

## Goals / Non-Goals

**Goals:**
- Update `esdiag` data models to include failure store information.
- Ensure failure store indices are enriched with data stream metadata.
- Maintain compatibility with older Elasticsearch versions that do not have failure stores.

**Non-Goals:**
- Adding new diagnostics specifically for failure store content.

## Decisions

### 1. Data Model Updates
We will add `FailureStore` and `FailureStoreIndex` structs to `src/processor/elasticsearch/data_stream/data.rs`. 
- `FailureStore` will contain `enabled`, `rollover_on_write`, `indices`, and `lifecycle`.
- `FailureStoreIndex` will likely match the structure of `IndexEntry` but will be defined separately if needed for clarity or future divergence.
- `DataStream` and `DataStreamDocument` will be updated to include an `Option<FailureStore>` field.

### 2. Lookup Enrichment
The `From<DataStreams> for Lookup<DataStreamDocument>` implementation in `src/processor/elasticsearch/data_stream/lookup.rs` will be modified.
Currently, it iterates over `data_stream.indices`. It will be updated to also iterate over `data_stream.failure_store.indices` (if present) and add them to the lookup table with the same `DataStreamDocument`.

### 3. Serde Handling
Use `#[skip_serializing_none]` and `Option` to ensure that missing `failure_store` fields (on older ES versions) do not cause issues.

## Risks / Trade-offs

- **[Risk] Mapping complexity** → The `failure_store` structure might evolve. 
  - **Mitigation**: Use `serde(flatten)` or `Option` for non-critical fields to remain flexible.
- **[Risk] Performance** → Adding more indices to the lookup.
  - **Mitigation**: Failure stores typically have very few indices compared to backing indices, so the overhead should be negligible.
