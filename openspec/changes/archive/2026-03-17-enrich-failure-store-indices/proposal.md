## Why

Failure store is a new Elasticsearch feature where failure indices are reported in a `failure_store.indices` field of the data stream. Currently, these indices are not enriched with data stream information (type, dataset, namespace) like backing indices are, which makes it harder to analyze them in the context of the data stream.

## What Changes

- Add `FailureStore` and `FailureStoreIndex` structs to the `DataStream` model.
- Update `DataStreamDocument` to include `failure_store` information.
- Ensure that indices belonging to a failure store are correctly identified and enriched with the same data stream metadata as regular backing indices.

## Capabilities

### New Capabilities
- `failure-store-enrichment`: Enrich failure store indices with data stream metadata (type, dataset, namespace).

### Modified Capabilities
- `data-stream-model`: Update the data stream model to support failure store fields.

## Impact

- `src/processor/elasticsearch/data_stream/data.rs`: New structs and updated `DataStream`/`DataStreamDocument`.
- `src/processor/elasticsearch/data_stream/lookup.rs`: Logic for enriching failure store indices.
- `src/processor/elasticsearch/indices_stats/processor.rs`: May need updates if it processes failure store indices.
