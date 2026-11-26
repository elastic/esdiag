## Why

Operators running ESDiag (especially in container/service deployments or long-running CLI jobs) currently have to deploy a separate log shipper to collect ESDiag's own application logs from stdout/stderr, and local file logging unconditionally writes to `~/.esdiag/last_run/esdiag.log`. Adding a native Elasticsearch logs exporter and an opt-in file log path lets ESDiag ship its own structured tracing events directly to Elasticsearch and control local log file persistence without external tooling (tracked in [elastic/esdiag#475](https://github.com/elastic/esdiag/issues/475)).

## What Changes

- **Target Elastic product**: Elasticsearch (log shipping destination via the `_bulk` API into a data stream, defaulting to `logs-esdiag-default`).
- **Affected surfaces**: Core tracing/logging layer, CLI startup/shutdown lifecycle, application configuration (`~/.esdiag/esdiag.yml`), and environment variable contract. No Web UI or diagnostic ETL processing logic changes.
- Add a native Elasticsearch `tracing_subscriber::Layer` that captures structured tracing events and ships them asynchronously in batches to an Elasticsearch cluster data stream (`logs-esdiag-default` by default, overridable via `ESDIAG_LOGS_DATA_STREAM`).
- Support configuring the remote Elasticsearch logs target via:
  - Environment variables: `ESDIAG_LOGS_URL` with optional `ESDIAG_LOGS_APIKEY` (or `ESDIAG_LOGS_USERNAME` / `ESDIAG_LOGS_PASSWORD`) and optional `ESDIAG_LOGS_INSECURE` for self-signed TLS, or
  - Saved host reuse: `ESDIAG_LOGS_HOST=<name>` environment variable or `logs.host` in `~/.esdiag/esdiag.yml`, resolving an existing saved Elasticsearch host with role `send` and its keystore credentials.
- Flush buffered remote log events cleanly before process exit with bounded shutdown timeouts, while never blocking or crashing the application if the remote logs cluster is unreachable.
- **BREAKING**: Replace the unconditional `~/.esdiag/last_run/esdiag.log` tracing file sink with opt-in daily-rotating file logging controlled by `ESDIAG_LOGS_PATH` (and optional `logs.path` in `~/.esdiag/esdiag.yml`), disabled (`None`) by default. When enabled, logs write to date-named files `esdiag-YYYY-MM-DD.log` inside the configured directory (defaulting to `~/.esdiag/logs` when a default path is requested), roll over to a new file each day, enforce configurable retention (`ESDIAG_LOGS_RETENTION_DAYS` / `logs.retention_days`, defaulting to `30` days), and gzip previous days' `.log` files into `.log.gz` on daily rollover in `serve` mode and on process exit for one-shot commands (never on startup). Update local stack compose log helpers to follow the configured log path or fall back cleanly when file logging is disabled.
- Document the new `ESDIAG_LOGS_*` environment variables and `esdiag.yml` `logs` section, and record the user-visible change in `CHANGELOG.md`.

## Capabilities

### New Capabilities

- `logs-exporter`: Native Elasticsearch tracing log exporter (target resolution from `ESDIAG_LOGS_*` env vars or saved `send`-role Elasticsearch hosts, batched async `_bulk` delivery to `logs-esdiag-default` / `ESDIAG_LOGS_DATA_STREAM`, recursion guard, and bounded graceful flush) plus opt-in daily-rotating file logging via `ESDIAG_LOGS_PATH` with configurable retention (`ESDIAG_LOGS_RETENTION_DAYS`, default 30 days) and gzip compression of previous days' logs on exit.

### Modified Capabilities

- `structured-tracing`: Extend tracing subscriber initialization to compose the optional remote Elasticsearch logs layer and replace unconditional `last_run/esdiag.log` file logging with opt-in `ESDIAG_LOGS_PATH` file logging.
- `application-configuration`: Add an optional non-secret `logs` section (`host`, `path`, and `retention_days`) to `~/.esdiag/esdiag.yml` and validate `logs.host` references against saved Elasticsearch `send`-role hosts.

## Impact

- `src/main.rs`: Update `init_tracing` and `async_main` to resolve logs target/path configuration, attach the optional Elasticsearch tracing layer and opt-in file layer, and flush the remote exporter before exit.
- `crates/esdiag-core/src/data/application_config.rs`: Add `LogsConfig` (`host: Option<String>`, `path: Option<PathBuf>`, `retention_days: Option<u32>`) to `ApplicationConfig` and extend `validate_references()`.
- `crates/esdiag-core/src/data/mod.rs` & `src/local.rs`: Replace `open_run_log` / `last_run/esdiag.log` assumptions with `ESDIAG_LOGS_PATH`-aware daily log file resolution.
- New module `src/logging/` (in the root `esdiag` crate, where `tracing-subscriber` lives): Target resolution, structured log event serialization (ECS-compatible `@timestamp`, `log.level`, `log.logger`, `message`, span/event fields), bounded channel + background Tokio batch flusher, `tracing_subscriber::Layer` implementation, and daily-rotating file writer with exit gzip compression and retention pruning.
- `docs/` and `CHANGELOG.md`: Document `ESDIAG_LOGS_URL`, `ESDIAG_LOGS_APIKEY`, `ESDIAG_LOGS_USERNAME`, `ESDIAG_LOGS_PASSWORD`, `ESDIAG_LOGS_INSECURE`, `ESDIAG_LOGS_HOST`, `ESDIAG_LOGS_DATA_STREAM`, `ESDIAG_LOGS_PATH`, `ESDIAG_LOGS_RETENTION_DAYS`, and `esdiag.yml` `logs` configuration.
