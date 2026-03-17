## 1. Error Type

- [x] 1.1 Add `ExporterError` enum with `RateLimited` and `Fatal(eyre::Report)` variants to `src/exporter/elasticsearch.rs`
- [x] 1.2 Update `parse_response` return type to `Result<BatchResponse, ExporterError>` and map HTTP 429 to `ExporterError::RateLimited` instead of returning `Err`

## 2. Retry Configuration

- [x] 2.1 Add `RetryConfig` struct with fields `max_attempts: u32`, `initial_ms: u64`, `max_ms: u64`
- [x] 2.2 Implement `RetryConfig::from_env()` reading `ESDIAG_EXPORT_RETRY_MAX`, `ESDIAG_EXPORT_RETRY_INITIAL_MS`, `ESDIAG_EXPORT_RETRY_MAX_MS` with defaults (5, 1000, 30000)

## 3. Retry Loop in `batch_send`

- [x] 3.1 Refactor `batch_send` to accept `&RetryConfig` and loop up to `max_attempts` times
- [x] 3.2 Implement exponential backoff with ±25% jitter: `delay = min(initial_ms * 2^attempt, max_ms)` with jitter applied using `rand`
- [x] 3.3 On `ExporterError::RateLimited`: log `warn!` with attempt number and sleep duration, sleep, increment retry counter, then retry
- [x] 3.4 On `ExporterError::Fatal`: propagate immediately without retry
- [x] 3.5 On exhaustion (all attempts were 429): log `error!` with batch index name and attempt count, return `Ok(BatchResponse)` with `errors` set and retries populated so the export run continues

## 4. Retry Metrics

- [x] 4.1 Thread the retry counter through to `BatchResponse.retries` so it reflects actual retry attempts made

## 5. Verification

- [x] 5.1 Run `cargo clippy` and resolve all warnings
- [x] 5.2 Run `cargo test` and ensure all existing tests pass
