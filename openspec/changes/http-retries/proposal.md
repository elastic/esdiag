## Why

When the Elasticsearch exporter sends bulk indexing batches to the output cluster, HTTP 429 (Too Many Requests) responses are currently treated as unrecoverable errors, causing silent batch loss. These responses indicate transient backpressure from the output cluster that should be retried.

## What Changes

- The `parse_response` function in the Elasticsearch exporter will detect HTTP 429 responses and signal retry rather than returning an error.
- The `batch_send` method will implement retry logic with exponential backoff when a 429 is received.
- The existing (always-zero) `BatchResponse.retries` field will be populated to track retry counts in export metrics.
- Retry behavior (max attempts, initial backoff, max backoff) will be configurable via environment variables following the existing `ESDIAG_*` pattern.

## Capabilities

### New Capabilities
- `es-exporter-retries`: Retry logic for bulk export batches when the output Elasticsearch cluster responds with HTTP 429, including exponential backoff and retry count tracking in export metrics.

### Modified Capabilities
<!-- No existing spec-level requirements are changing -->

## Impact

- **Core logic**: `src/exporter/elasticsearch.rs` — `batch_send`, `parse_response`
- **Metrics**: `src/processor/diagnostic/report.rs` — `BatchResponse.retries` populated
- **Configuration**: New env vars `ESDIAG_EXPORT_RETRY_MAX`, `ESDIAG_EXPORT_RETRY_INITIAL_MS`, `ESDIAG_EXPORT_RETRY_MAX_MS`
- **No breaking changes** — new env vars are opt-in with sensible defaults
