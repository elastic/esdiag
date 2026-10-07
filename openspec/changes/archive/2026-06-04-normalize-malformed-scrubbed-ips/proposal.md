## Why

Scrubbed diagnostics can replace real node addresses with deterministic but invalid IPv4 values (for example `432.359.345.528`). These values break `ip` field handling and can cause partial ingest or missing node visualizations even when the diagnostic is otherwise processable.

esdiag needs a deterministic, low-overhead, and auditable normalization strategy so malformed scrubbed IPs do not block node-level analytics.

## What Changes

- Add a **scrubbed archive receiver stage** in the read path: when `Receiver` detects a scrubbed diagnostic bundle, it normalizes malformed IP-like values before handing data to processors.
- Add dual activation controls for scrub handling:
  - auto mode when archive filename/path contains `scrubbed`
  - explicit activation control in each execution channel:
    - CLI channel uses `--scrubbed BOOL`
    - UI upload channel uses scrub checkbox
- Keep processors unchanged as consumers of normalized input (no per-processor normalization logic by default).
- Add a canonical malformed-IPv4 normalization path in esdiag using octet transformation (`octet % 255`) with stable, deterministic output.
- Apply normalization to all known node-related address surfaces (`ip`, `host`, `transport_address`, `publish_address`, `bound_address`, and nested transport/http address fields) while preserving non-address semantics.
- Do not add an ingest-fallback retry path; normalization is handled in the receiver path before processor execution.
- Add diagnostics/telemetry for normalization events (count, source field, sample values redacted) so support can trace why values changed.
- Add a strict fallback behavior: if a value still cannot be normalized safely, route to non-fatal handling (drop field or tag document) rather than failing entire node ingestion.
- Add tests and fixtures covering malformed scrubbed bundles and validating that node summary dashboards populate after ingest.
- Add non-mangling safety tests to prove normal diagnostics are unchanged and only targeted malformed scrubbed IP fields are transformed.
- Add node-name correction planning in node lookup rename defaults:
  - detect scrubbed node names that match 19-character lowercase hex
  - preserve readability by keeping the last 4 chars of the original scrubbed name in the renamed output
  - use existing rename logic shape, but replace the numeric segment with the original scrubbed name's last 4 chars
- Add a dev ingest verification workflow that runs real `esdiag process` ingestion and hard-fails on bulk/index mapping conflicts.
  - conflict indicators are recorded and evaluated in debug mode output/artifacts

## Capabilities

### New Capabilities
- `scrubbed-archive-receiver`: Receiver-level scrubbed archive detection and pre-processor normalization pass.
- `malformed-ip-normalization`: Deterministic normalization and validation of malformed scrubbed IPv4 values across node-centric diagnostic payloads.
- `scrubbed-mode-controls`: Auto-detect plus explicit controls per channel (CLI flag or UI checkbox).
- `normalization-observability`: Structured counters/tags for when and where normalization was applied.
- `node-name-humanization`: Deterministic tier-based rename with retained scrubbed suffix (last 4 chars) for operator readability.
- `dev-ingest-validation`: Development workflow tests that ingest into Elasticsearch and assert clean bulk results with no mapping conflicts.

### Modified Capabilities
- None currently (no existing OpenSpec capabilities are defined yet in this repository).

## Impact

- **Affected code paths**
  - `src/receiver/mod.rs`
  - `src/receiver/archive/file.rs`
  - `src/receiver/archive/bytes.rs`
  - `src/processor/elasticsearch/nodes/data.rs`
  - `src/processor/elasticsearch/nodes/lookup.rs` (rename default updates for scrubbed names)
  - `src/processor/elasticsearch/nodes_stats/data.rs`
  - `src/processor/elasticsearch/nodes_stats/processor.rs`
  - `src/receiver/scrub.rs`
  - `src/server/file_upload.rs` (checkbox path)
  - `src/main.rs` / CLI argument surface (flag path)
- **Data/Schema**
  - Potential updates to mappings/pipelines for `metrics-node-esdiag` and related node datasets.
  - Additional optional metadata fields/tags for normalization observability.
- **Operational**
  - Receiver-first strategy is the default and only normalization path.
  - No ingest-pipeline fallback dependency is planned for this change.

### Ryan Questions / Decision Points For Review

- **Default strategy:** Confirm receiver-first as the only path. no fallback, this will be handled by receiver
- **Detection contract:** Confirm default auto-detect as filename/path contains `scrubbed`, with manual checkbox/flag override. yes
- **Override precedence:** CLI and UI are separate execution channels (either/or); no cross-channel precedence required.
- **Non-mangling guarantee:** Which fixtures become hard gates to prove untouched behavior for non-scrubbed inputs? IPs are still valid IPs
- **Safety boundary:** Should normalization run only on explicitly recognized IP fields, or on any string that parses as malformed IPv4?  explicetly only
- **Collision tolerance:** Is `octet % 255` acceptable given potential collisions, or do we need an additional deterministic tie-breaker to reduce collision risk? if we are deterministcally determining IP, how would that even happen? so i think we are safe there
- **Failure mode:** If normalization still yields unusable output for a field, should esdiag drop the field, tag and keep document, or route to a dead-letter stream? cluster will handle that
- **Performance target:** What overhead budget is acceptable for per-document normalization in large diagnostics?  memory usage is a bigger concern, optimze,a 20% memory usage RSS on the scrubbed one compared to the unscrubbed
- **Governance:** Do we require an opt-out/opt-in switch for environments that need original scrubbed values preserved?  NA, not needed
- **Auditability:** What minimum telemetry is required for support (counts only vs per-field counters vs sampled examples)? no telemetry needed, just debug logging per unscrubbed file read
