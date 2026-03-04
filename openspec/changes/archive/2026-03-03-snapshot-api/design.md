## Context
ESDiag needs snapshot repository and snapshot metadata for backup/restore diagnostics. The previous proposal treated the responses as fully materialized payloads, which is risky for large clusters where `_snapshot/_all/_all` can be very large.

## Goals / Non-Goals

**Goals:**
- Collect snapshot repositories from `_snapshot`.
- Collect snapshot records from `_snapshot/_all/_all`.
- Use streaming deserialization and streaming export patterns aligned with `indices_stats`.
- Preserve structured document output suitable for stable indexing and analysis.

**Non-Goals:**
- Management of snapshots (creation, deletion, restore).
- Deep analysis of snapshot content; the focus is on the metadata and state of the snapshots and repositories.

## Decisions

### Decision: Use `indices_stats`-style streaming path for snapshots
`Snapshots` MUST implement `StreamingDataSource` and support progressive item emission during JSON decode, rather than loading the full response into memory first.

**Rationale:** `indices_stats` already demonstrates the expected architecture in this codebase for large payloads:
- stream parse items from the datasource;
- process items incrementally;
- ship output through document channels.

### Decision: Export snapshot documents through document channels
Snapshot export MUST implement `StreamingDocumentExporter` and write documents using exporter document channels with bounded buffering and batched sends.
Snapshots MUST target `logs-snapshot-esdiag`, and repository settings MUST target `settings-repository-esdiag`.

**Rationale:** This makes backpressure explicit and avoids building a full in-memory document vector before export.

### Decision: Keep repositories and snapshots in one module, but with separate paths
The module remains `src/processor/elasticsearch/snapshots/`, but repositories and snapshots are processed through separate data/export flows. Repository collection may remain non-streaming if payload sizes are typically small, while snapshot collection is required to stream.

**Rationale:** Maintains cohesion of snapshot-related APIs while applying stricter behavior only where size risk is highest.

## Risks / Trade-offs

- **[Risk] More implementation complexity**: Streaming deserialize and channelized export are more complex than `Vec<Value>` processing.
  - **Mitigation**: Reuse `indices_stats` architecture and testing style.
- **[Risk] Partial stream failures**: Bad entries could occur mid-stream.
  - **Mitigation**: Log per-item failures, continue processing remaining entries, and reflect errors in processor summary.
- **[Trade-off] Typed breadth vs maintainability**: Fully exhaustive typed models may be costly for evolving snapshot payloads.
  - **Mitigation**: Define a required stable typed subset for critical fields and allow controlled passthrough for non-critical fields.
- **[Trade-off] Date extraction from names**: Snapshot naming is convention-based, not guaranteed.
  - **Mitigation**: Extract `snapshot.date` only when a `YYYY.MM.DD` token exists; otherwise leave it null/absent.
