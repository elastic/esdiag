## Context

The Elasticsearch exporter (`src/exporter/elasticsearch.rs`) sends document batches to an output cluster via the bulk API. When the output cluster is under load it returns HTTP 429 (Too Many Requests). Currently `parse_response` converts a 429 into an `Err`, which propagates up and causes the batch task to fail silently (logged as a warning, documents lost).

The `BatchResponse` struct already has an unused `retries: u16` field, signalling that retry tracking was anticipated but never implemented.

## Goals / Non-Goals

**Goals:**
- Retry 429 responses with exponential backoff until the batch succeeds or a configurable limit is reached.
- Populate `BatchResponse.retries` so retry counts appear in export metrics.
- Make retry parameters configurable via `ESDIAG_*` environment variables with sensible defaults.

**Non-Goals:**
- Retrying other transient errors (5xx, timeouts) — scope limited to 429 per the issue.
- Changing the retry strategy for the collection side (already handled by `collection-execution` spec).
- Circuit-breaker or persistent queue semantics.

## Decisions

### Decision: Retry loop inside `batch_send`, not `parse_response`

`parse_response` is a pure response-to-result transformer. Retrying there would require it to hold state (attempt count, sleep handles) and drive async I/O, which contradicts its role.

Instead, `batch_send` drives the loop: call the bulk API, call `parse_response`, inspect the result. A new error variant (or a sentinel `BatchResponse` field) indicates "retryable" vs "fatal".

**Alternative considered**: Introduce a wrapper at the semaphore-task level (`async_batch_tx`). Rejected because it duplicates the batch data across clones unnecessarily and increases complexity in the task-spawning path.

### Decision: New `ExporterError` type to distinguish 429 from fatal errors

`parse_response` returns `Result<BatchResponse>`. To signal a retryable 429 without a new return type, the simplest approach is a dedicated error variant that `batch_send` can pattern-match on:

```rust
#[derive(Debug)]
enum ExporterError {
    RateLimited,
    Fatal(eyre::Report),
}

impl std::fmt::Display for ExporterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExporterError::RateLimited => write!(f, "http 429 too many requests"),
            ExporterError::Fatal(e) => write!(f, "{e}"),
        }
    }
}
```

`batch_send` catches `RateLimited`, sleeps, and retries. All other errors propagate immediately.

**Alternative considered**: A boolean flag on `BatchResponse`. Rejected because it conflates response data with control flow and requires a valid `BatchResponse` even on failure.

### Decision: Exponential backoff with jitter, capped at a configurable maximum

Standard approach for rate-limit retry. Initial delay doubles each attempt (2^n × initial_ms), with ±25% random jitter to spread concurrent retries. Cap prevents indefinite blocking.

Environment variables (with defaults):
- `ESDIAG_EXPORT_RETRY_MAX` — maximum retries after the initial attempt (default: `5`, giving 6 total attempts)
- `ESDIAG_EXPORT_RETRY_INITIAL_MS` — initial backoff in ms (default: `1000`)
- `ESDIAG_EXPORT_RETRY_MAX_MS` — cap on a single backoff sleep in ms (default: `30000`)

**Alternative considered**: Fixed delay. Rejected — fixed delays can cluster retries from concurrent tasks.

### Decision: Log a warning on each retry, error on exhaustion

Consistent with existing exporter logging style. Each retry emits `log::warn!` with the attempt number and sleep duration. If all attempts are exhausted the batch fails with `log::error!`, matching current fatal-error behaviour.

## Risks / Trade-offs

- **Increased latency on export** → Mitigation: backoff is bounded by `ESDIAG_EXPORT_RETRY_MAX_MS`; the semaphore still limits total concurrent tasks so blocked retry tasks reduce throughput rather than piling up unbounded.
- **Batch data clone for retry** → `batch_send` serializes docs to `Arc<serde_json::Value>` once upfront, then `Arc::clone`s the references for each attempt. This avoids both re-serialization and deep-copying the JSON payload per retry, at the cost of holding the serialized batch in memory for the duration of the retry loop — acceptable given current default batch sizes.
- **Silent data loss on exhaustion** → This is the same behaviour as today (warning, continue). Operators who need zero-loss semantics should reduce write load or lower `ESDIAG_OUTPUT_TASK_LIMIT`.

## Migration Plan

No migration required. The change is additive: new env vars are optional, default behaviour on non-429 responses is unchanged, and the `retries` field was already serialised (always 0) in prior output so downstream consumers will see the field populated rather than absent.
