## Context
ESDiag currently lacks snapshot and repository information in its Elasticsearch diagnostics. This change introduces the necessary data structures and processing logic to collect and export this information using the project's existing trait-driven architecture.

## Goals / Non-Goals

**Goals:**
- Implement `DataSource` for Elasticsearch Snapshot Repository (`_snapshot`) and Snapshots (`_snapshot/_all/_all`) APIs.
- Create structured Rust models for the responses from these APIs using `serde`.
- Integrate the new data sources into the `ElasticsearchDiagnostic` processor to ensure they are collected during diagnostic runs.

**Non-Goals:**
- Management of snapshots (creation, deletion, restore).
- Deep analysis of snapshot content; the focus is on the metadata and state of the snapshots and repositories.

## Decisions

### Decision: Separate Modules for Repositories and Snapshots
We will create a unified `snapshots` module that handles both repositories and snapshot details, rather than two separate top-level modules.

**Rationale:** These two APIs are logically grouped under the snapshotting functionality. Within the `snapshots` module, we will define separate `DataSource` implementations for each endpoint.

### Decision: Module Structure
The new code will be located in `src/processor/elasticsearch/snapshots/`, containing:
- `mod.rs`: Module exports.
- `data.rs`: Serde-compatible structs for API responses.
- `processor.rs`: Implementation of the data collection and export logic.

**Rationale:** This maintains consistency with existing Elasticsearch processors like `nodes_stats` and `indices_settings`.

### Decision: Index Templates for Snapshots
We will add index templates for the new data streams to ensure they are correctly mapped in Elasticsearch.

**Rationale:** Proper mappings (e.g., keyword vs text, date formats) are essential for efficient querying and visualization of snapshot data.

## Risks / Trade-offs

- **[Risk] Large Response Size** → Clusters with a very high number of snapshots (thousands) may return a large JSON payload from `_snapshot/_all/_all`.
  - **Mitigation**: The system is designed to handle large diagnostic documents, but we should ensure the deserialization is efficient.
- **[Trade-off] All repositories vs specific repositories** → We chose `_all/_all` to get everything in one call.
  - **Mitigation**: This is standard for diagnostic collection to ensure no data is missed.
