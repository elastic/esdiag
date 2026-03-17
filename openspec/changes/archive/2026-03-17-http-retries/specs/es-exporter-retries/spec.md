## ADDED Requirements

### Requirement: Retry Bulk Export on HTTP 429
The Elasticsearch exporter SHALL retry a failed bulk batch when the output cluster responds with HTTP 429 (Too Many Requests). Retries MUST use exponential backoff with jitter. The maximum number of attempts and backoff bounds MUST be configurable via environment variables. After exhausting all attempts the exporter MUST log an error and continue processing remaining batches.

#### Scenario: 429 response triggers retry with backoff
- **GIVEN** the exporter sends a bulk batch to the output cluster
- **WHEN** the cluster responds with HTTP 429
- **THEN** the exporter logs a warning including the attempt number and next sleep duration
- **AND** the exporter waits for the computed backoff period before retrying the same batch
- **AND** the exporter resends the identical batch on the next attempt

#### Scenario: Batch succeeds after one or more retries
- **GIVEN** the exporter has retried a batch at least once due to HTTP 429
- **WHEN** a subsequent attempt receives HTTP 200
- **THEN** the batch is considered successfully delivered
- **AND** `BatchResponse.retries` reflects the number of retry attempts made

#### Scenario: All retry attempts exhausted
- **GIVEN** the exporter has retried a batch the maximum number of times (default 5)
- **WHEN** every attempt receives HTTP 429
- **THEN** the exporter logs an error indicating the batch was dropped after N attempts
- **AND** the exporter continues processing the next pending batch without aborting the export run

#### Scenario: Non-429 errors are not retried
- **GIVEN** the exporter sends a bulk batch
- **WHEN** the cluster responds with HTTP 400, 401, 403, 404, 413, or 5xx
- **THEN** the exporter treats the response as a fatal error immediately
- **AND** no retry is attempted

### Requirement: Retry Configuration via Environment Variables
The retry behaviour of the Elasticsearch exporter SHALL be configurable via environment variables at process startup. All variables MUST have sensible defaults so that no configuration is required for standard operation.

#### Scenario: Default configuration used when no env vars set
- **GIVEN** none of `ESDIAG_EXPORT_RETRY_MAX`, `ESDIAG_EXPORT_RETRY_INITIAL_MS`, or `ESDIAG_EXPORT_RETRY_MAX_MS` are set
- **WHEN** the exporter initialises
- **THEN** it uses a maximum of 5 retry attempts, 1000 ms initial backoff, and 30000 ms maximum backoff per sleep

#### Scenario: Custom retry limit applied
- **GIVEN** `ESDIAG_EXPORT_RETRY_MAX=3` is set in the environment
- **WHEN** a bulk batch receives repeated HTTP 429 responses
- **THEN** the exporter retries at most 3 times before logging an error and dropping the batch

### Requirement: Retry Count Reflected in Export Metrics
The `BatchResponse` MUST record the number of retry attempts made for each batch. This count SHALL be included in export summary metrics.

#### Scenario: Retry count populated after retried success
- **GIVEN** a batch required 2 retries before succeeding
- **WHEN** the batch completes and `BatchResponse` is returned
- **THEN** `BatchResponse.retries` equals 2
