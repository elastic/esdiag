## Purpose

Defines native exporting of ESDiag's own structured tracing logs to an Elasticsearch cluster and opt-in local log file persistence controlled by `ESDIAG_LOGS_PATH` and `~/.esdiag/esdiag.yml`.

## ADDED Requirements

### Requirement: Remote Logs Target Resolution
The system SHALL resolve an optional remote Elasticsearch logs target on startup using the following precedence:
1. Environment URL configuration: `ESDIAG_LOGS_URL` with optional authentication from `ESDIAG_LOGS_APIKEY`, or `ESDIAG_LOGS_USERNAME` and `ESDIAG_LOGS_PASSWORD` together, and optional `ESDIAG_LOGS_INSECURE` (`true`/`1` to disable TLS certificate verification; strict TLS by default).
2. Environment saved-host reference: `ESDIAG_LOGS_HOST` naming a saved Elasticsearch host in `hosts.yml` with role `send`.
3. Persisted saved-host reference: `logs.host` in `~/.esdiag/esdiag.yml` naming a saved Elasticsearch host in `hosts.yml` with role `send`.
4. Disabled (no remote logs exporter) when none of the above are configured.

The system MUST NOT combine credentials from one source with a URL from another source, and MUST NOT fall back to `ESDIAG_OUTPUT_URL` or `output.default` when no logs target is configured.

#### Scenario: Environment URL and API key configure remote logs target
- **GIVEN** `ESDIAG_LOGS_URL` is set to `https://logs.example:9200`
- **AND** `ESDIAG_LOGS_APIKEY` is set to a non-empty value
- **WHEN** the application resolves the remote logs target at startup
- **THEN** the logs exporter targets `https://logs.example:9200` using API key authentication
- **AND** any persisted `logs.host` in `esdiag.yml` is ignored

#### Scenario: Environment URL with conflicting authentication fails configuration
- **GIVEN** `ESDIAG_LOGS_URL` is set
- **AND** both `ESDIAG_LOGS_APIKEY` and `ESDIAG_LOGS_USERNAME` are set
- **WHEN** the application resolves the remote logs target at startup
- **THEN** target resolution fails with a configuration error
- **AND** the application logs a warning to stderr and continues running without remote log shipping

#### Scenario: Environment saved-host reference selects saved Elasticsearch host
- **GIVEN** `ESDIAG_LOGS_URL` is unset
- **AND** `ESDIAG_LOGS_HOST` is set to `ops-es`
- **AND** `ops-es` is a saved Elasticsearch host with role `send`
- **WHEN** the application resolves the remote logs target at startup
- **THEN** the logs exporter targets `ops-es` and resolves its output credentials through the existing keystore

#### Scenario: Persisted logs.host selects saved Elasticsearch host
- **GIVEN** `ESDIAG_LOGS_URL` and `ESDIAG_LOGS_HOST` are unset
- **AND** `esdiag.yml` sets `logs.host` to `ops-es`
- **AND** `ops-es` is a saved Elasticsearch host with role `send`
- **WHEN** the application resolves the remote logs target at startup
- **THEN** the logs exporter targets `ops-es` and resolves its output credentials through the existing keystore

#### Scenario: Saved host without send role or non-Elasticsearch application is rejected
- **GIVEN** `ESDIAG_LOGS_HOST` references a saved host that is not an Elasticsearch host or lacks role `send`
- **WHEN** the application resolves the remote logs target at startup
- **THEN** target resolution fails with an actionable error
- **AND** the application logs a warning to stderr and continues running without remote log shipping

#### Scenario: No logs target configured disables remote shipping
- **GIVEN** `ESDIAG_LOGS_URL`, `ESDIAG_LOGS_HOST`, and `esdiag.yml` `logs.host` are all unset
- **AND** `ESDIAG_OUTPUT_URL` is set
- **WHEN** the application resolves the remote logs target at startup
- **THEN** no remote logs exporter is created

#### Scenario: ESDIAG_LOGS_INSECURE disables TLS verification for environment URL
- **GIVEN** `ESDIAG_LOGS_URL` is set to an HTTPS endpoint with a self-signed certificate
- **AND** `ESDIAG_LOGS_INSECURE=true` is set
- **WHEN** the application constructs the remote logs client
- **THEN** TLS certificate verification is disabled for the logs client
### Requirement: Target Data Stream Selection
The system SHALL ship log documents using Elasticsearch `_bulk` `create` operations targeting the data stream `logs-esdiag-default` by default, and SHALL allow overriding the target data stream via `ESDIAG_LOGS_DATA_STREAM`.

#### Scenario: Default data stream is used
- **GIVEN** a remote logs target is configured
- **AND** `ESDIAG_LOGS_DATA_STREAM` is unset
- **WHEN** the logs exporter flushes a batch of log events
- **THEN** bulk `create` actions target `logs-esdiag-default`

#### Scenario: Custom data stream overrides default
- **GIVEN** a remote logs target is configured
- **AND** `ESDIAG_LOGS_DATA_STREAM` is set to `logs-esdiag-prod`
- **WHEN** the logs exporter flushes a batch of log events
- **THEN** bulk `create` actions target `logs-esdiag-prod`

### Requirement: Structured Log Document Schema
Each shipped log document SHALL be a JSON object containing at minimum `@timestamp` (RFC 3339 UTC), `log.level`, `log.logger` (tracing target), `message`, `service.name` (`esdiag`), `service.version`, and any structured event fields and active span context.

#### Scenario: Tracing event is serialized as an ECS-compatible document
- **GIVEN** a remote logs exporter is active
- **WHEN** an instrumented span emits `tracing::info!(job_id = "abc", "processing started")`
- **THEN** the serialized document includes `@timestamp`, `log.level` (`info`), `log.logger`, `message` (`processing started`), `service.name` (`esdiag`), `service.version`, and the `job_id` field

### Requirement: Asynchronous Batched Delivery and Failure Isolation
The remote logs exporter SHALL enqueue log events into a bounded non-blocking queue and flush them asynchronously on a background task (on batch size threshold or periodic flush interval). The exporter MUST NOT emit tracing events from its own transport path that re-enter the exporter (preventing feedback loops). If the queue is full or the remote cluster is unreachable or returns errors, the exporter MUST drop events or report delivery errors to stderr without blocking, panicking, or failing the primary ESDiag command.

#### Scenario: Unreachable logs cluster does not fail command execution
- **GIVEN** `ESDIAG_LOGS_URL` points to an unreachable endpoint
- **WHEN** ESDiag runs a command that succeeds locally
- **THEN** the command exits with success
- **AND** log delivery failures do not panic or hang the process

#### Scenario: Exporter internal transport logs do not recurse
- **GIVEN** the remote logs exporter is active at `debug` level
- **WHEN** the exporter sends a `_bulk` HTTP request to the logs cluster
- **THEN** HTTP client / exporter internal events do not recursively enqueue new log documents

### Requirement: Graceful Flush on Shutdown
Before process exit, the application SHALL signal the remote logs exporter to flush remaining buffered events and wait up to a bounded shutdown timeout before terminating.

#### Scenario: Short-lived CLI command flushes logs before exit
- **GIVEN** a remote logs exporter is configured and reachable
- **WHEN** a finite CLI command emits tracing events and completes
- **THEN** buffered log events are flushed to Elasticsearch before the process exits

### Requirement: Opt-In Daily File Logging Path
The system SHALL disable file logging by default (`None`). File logging SHALL be enabled only when `ESDIAG_LOGS_PATH` is set or `logs.path` is configured in `~/.esdiag/esdiag.yml`, with `ESDIAG_LOGS_PATH` taking precedence over `logs.path`. Setting `ESDIAG_LOGS_PATH` to an empty string SHALL disable file logging even if `logs.path` is set in `esdiag.yml`. When a default directory is requested via configuration without an explicit path, it SHALL default to `~/.esdiag/logs` (respecting `ESDIAG_HOME`).

When file logging is enabled, the system SHALL create the directory if needed and append logs to a date-named file `esdiag-YYYY-MM-DD.log` (UTC date of creation) inside that directory with owner-only permissions (`0600` on Unix). For long-running processes, the file writer SHALL roll over automatically to a new `esdiag-YYYY-MM-DD.log` file when the calendar day changes.

#### Scenario: File logging is disabled by default
- **GIVEN** `ESDIAG_LOGS_PATH` is unset
- **AND** `esdiag.yml` does not configure `logs.path`
- **WHEN** ESDiag starts and emits tracing events
- **THEN** no log file is created on disk

#### Scenario: ESDIAG_LOGS_PATH enables daily file logging in the specified directory
- **GIVEN** `ESDIAG_LOGS_PATH` is set to `/tmp/custom-esdiag-logs`
- **AND** the current UTC date is `2026-11-24`
- **WHEN** ESDiag starts and emits tracing events
- **THEN** `/tmp/custom-esdiag-logs` is created if absent
- **AND** tracing events are appended to `/tmp/custom-esdiag-logs/esdiag-2026-11-24.log` with owner-only permissions on Unix

#### Scenario: Long-running process rolls over to a new file on day change
- **GIVEN** file logging is enabled and writing to `esdiag-2026-11-24.log`
- **WHEN** the UTC date advances to `2026-11-25` and a new tracing event is emitted
- **THEN** the writer opens and appends to `esdiag-2026-11-25.log`
- **AND** no further events are written to `esdiag-2026-11-24.log`

#### Scenario: Empty ESDIAG_LOGS_PATH disables persisted logs.path
- **GIVEN** `esdiag.yml` configures `logs.path`
- **AND** `ESDIAG_LOGS_PATH` is set to an empty string
- **WHEN** ESDiag starts
- **THEN** file logging is disabled and no log file is written

### Requirement: File Log Compression and Retention
The system SHALL support configurable file log retention in days via `ESDIAG_LOGS_RETENTION_DAYS` (taking precedence over `logs.retention_days` in `~/.esdiag/esdiag.yml`), defaulting to `30` days.

When file logging is enabled, the system MUST NOT compress log files during startup. Compression of previous days' uncompressed `esdiag-YYYY-MM-DD.log` files (date strictly earlier than the current UTC date) into `esdiag-YYYY-MM-DD.log.gz` with owner-only permissions (`0600` on Unix, removing the uncompressed `.log` only after compression succeeds) SHALL run:
- On daily rollover for long-running `serve` mode (asynchronously so logging writes are not blocked), and
- On process exit for one-shot CLI executions (and on `serve` shutdown for any remaining prior-day files).

The current day's `esdiag-YYYY-MM-DD.log` file MUST remain uncompressed so subsequent runs on the same day can continue appending.

On daily rollover and on process exit, the system SHALL delete any `esdiag-YYYY-MM-DD.log` or `esdiag-YYYY-MM-DD.log.gz` files in the logs directory whose filename date is older than the configured retention window (strictly older than `today - retention_days`). Non-matching files in the logs directory MUST NOT be modified or deleted.

#### Scenario: Previous days' log files are gzipped on exit for one-shot commands, not on startup
- **GIVEN** the logs directory contains `esdiag-2026-11-23.log` and `esdiag-2026-11-24.log`
- **AND** the current UTC date is `2026-11-24`
- **WHEN** a one-shot ESDiag command starts
- **THEN** `esdiag-2026-11-23.log` is NOT compressed during startup
- **WHEN** the command exits cleanly
- **THEN** `esdiag-2026-11-23.log` is compressed to `esdiag-2026-11-23.log.gz` and the uncompressed `esdiag-2026-11-23.log` is removed
- **AND** `esdiag-2026-11-24.log` remains uncompressed

#### Scenario: Long-running serve mode gzips previous day's log file on daily rollover
- **GIVEN** `esdiag serve` is running with file logging enabled and writing to `esdiag-2026-11-24.log`
- **WHEN** the UTC date advances to `2026-11-25` and a new tracing event is emitted
- **THEN** the writer rolls over to `esdiag-2026-11-25.log`
- **AND** `esdiag-2026-11-24.log` is compressed to `esdiag-2026-11-24.log.gz` in the background and removed once compressed


#### Scenario: Log files older than retention window are pruned
- **GIVEN** retention is configured to `30` days (default) and the current UTC date is `2026-11-24`
- **AND** the logs directory contains `esdiag-2026-10-20.log.gz`, `esdiag-2026-10-25.log.gz`, and an unrelated `notes.txt`
- **WHEN** retention cleanup runs on rollover or exit
- **THEN** `esdiag-2026-10-20.log.gz` (35 days old) is deleted
- **AND** `esdiag-2026-10-25.log.gz` (30 days old) and `notes.txt` are preserved

#### Scenario: ESDIAG_LOGS_RETENTION_DAYS overrides esdiag.yml retention_days
- **GIVEN** `esdiag.yml` sets `logs.retention_days: 14`
- **AND** `ESDIAG_LOGS_RETENTION_DAYS` is set to `7`
- **WHEN** retention cleanup runs
- **THEN** files older than 7 days are deleted
