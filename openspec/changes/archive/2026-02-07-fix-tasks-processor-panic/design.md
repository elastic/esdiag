## Context

The `Tasks` processor in `esdiag` enriches raw task data from Elasticsearch with node metadata. This enrichment is currently performed using an `.expect()` call on the node lookup. If the node ID provided in the tasks response is not present in the nodes lookup (which can happen in environments like Elasticsearch Serverless or due to inconsistent diagnostic capture), the processor panics.

## Goals / Non-Goals

**Goals:**
- Eliminate the panic in the tasks processor when node metadata is missing.
- Maintain the ability to export task data even when it cannot be enriched with node details.
- Provide clear logging when enrichment fails to assist in debugging diagnostic capture issues.

**Non-Goals:**
- Implementing a retry mechanism for node lookups.
- Changing the fundamental structure of the `EnrichedTask` beyond making the node optional.

## Decisions

### 1. Make `EnrichedTask.node` optional
Instead of requiring a `NodeDocument`, the `EnrichedTask` struct will be updated to hold an `Option<NodeDocument>`.
- **Rationale**: This is the most idiomatic way in Rust to handle potentially missing data. It allows `serde` to handle the field (omitting it if `None`) and avoids the need for placeholder data.

### 2. Use `Option.cloned()` for node lookup
The lookup in the parallel iterator will be updated to use `cloned()` instead of `cloned().expect()`.
- **Rationale**: This allows the map operation to continue regardless of whether the node was found.

### 3. Log a warning on missing node
A `log::warn!` or `log::error!` message will be added when a node ID cannot be found in the lookup.
- **Rationale**: While we want to continue processing, it's important to record that the enrichment was incomplete, as this might indicate a bug in the diagnostic collector or an unsupported environment configuration.

## Risks / Trade-offs

- **Risk**: Downstream consumers of the exported data might expect the `node` field to always be present.
- **Mitigation**: Elasticsearch data streams are generally resilient to missing fields, and the `metrics-task-esdiag` data stream mapping should already handle optional fields.

- **Risk**: Excessive logging if many tasks are missing node metadata.
- **Mitigation**: We could consider rate-limiting the log message or logging it once per node ID per processing run. However, given the typical scale of diagnostics, a simple log message is likely sufficient for now.
