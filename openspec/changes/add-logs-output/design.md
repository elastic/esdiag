## Context

ESDiag currently initializes a `tracing_subscriber::registry()` in `src/main.rs::init_tracing()` with two layers:
1. A stderr `fmt::layer()` filtered by `resolve_tracing_filter(&cli)`.
2. A file `fmt::layer()` that unconditionally opens `~/.esdiag/last_run/esdiag.log` (`esdiag::data::open_run_log(true)`) for non-server commands.

The application already ships processed diagnostic documents to Elasticsearch via `ElasticsearchClient` (`crates/esdiag-core/src/client/elasticsearch.rs`) and resolves saved hosts with roles and encrypted keystore credentials via `KnownHost` (`crates/esdiag-core/src/data/known_host.rs`) and `ApplicationConfig` (`crates/esdiag-core/src/data/application_config.rs`). See `proposal.md` for motivation and user-facing scope.

Constraints:
- `init_tracing` runs inside `async_main()` on the Tokio multi-threaded runtime, before any command executes.
- `tracing_subscriber::Layer::on_event` is synchronous and may be called from any thread (including Tokio worker threads, blocking pools, and panic hooks). It must never block on network I/O.
- `Cargo.toml` already includes `tracing`, `tracing-subscriber`, `elasticsearch`, `reqwest`, `tokio`, `serde_json`, `chrono`, and `url`. No new crates are required.
- Feature flags: `keystore`, `server`, `agent`, `setup` are optional; lite builds may omit `keystore` or `server`. The logs exporter must compile cleanly under both default (`full`) and minimal (`--no-default-features`) feature sets.

## Goals / Non-Goals

**Goals:**
- Provide a self-contained `src/logging/` module implementing:
  - Remote Elasticsearch logs target resolution (`ESDIAG_LOGS_URL` + auth env vars, `ESDIAG_LOGS_HOST`, or `esdiag.yml` `logs.host`).
  - Opt-in daily-rotating file logging (`ESDIAG_LOGS_PATH`, `esdiag.yml` `logs.path`, default `None`, default directory helper `~/.esdiag/logs`) writing to `esdiag-YYYY-MM-DD.log`, with configurable retention (`ESDIAG_LOGS_RETENTION_DAYS` / `logs.retention_days`, default `30` days) and gzip compression of previous days' logs on exit.
  - A non-blocking `tracing_subscriber::Layer` that serializes events + span context into ECS-compatible JSON documents and pushes them onto a bounded `tokio::sync::mpsc` channel.
  - A background Tokio worker that batches documents and ships them via Elasticsearch `_bulk` `create` operations to `logs-esdiag-default` (or `ESDIAG_LOGS_DATA_STREAM`).
  - A `LogsGuard` handle returned by `init_tracing` that performs a bounded async flush of remote logs and runs file log exit maintenance (gzipping previous days' `.log` files + retention pruning) before `async_main()` returns.
- Preserve strict isolation: remote log configuration errors or cluster outages degrade gracefully to stderr warnings and never fail the ESDiag command.
- Prevent recursive logging loops when the Elasticsearch/reqwest/hyper transport emits its own tracing events.

**Non-Goals:**
- Shipping diagnostic documents or replacing `ElasticsearchExporter` in `crates/esdiag-core/src/exporter/elasticsearch.rs` (that handles ETL diagnostic batches, retries, and `DiagnosticReport` accounting).
- Adding custom Elasticsearch index templates or Kibana dashboards for ESDiag application logs (`logs-esdiag-default` relies on Elasticsearch's built-in `logs-*-*` data stream template).
- Size-based intra-day log rotation (rotation is strictly daily by UTC date).
- Web UI controls for editing `logs.host`, `logs.path`, or `logs.retention_days` (CLI/config/env only for now).

## Decisions

### 1. New `src/logging/` module layout

Create `src/logging/mod.rs` (plus focused submodules: `config.rs`, `file.rs`, `layer.rs`, `worker.rs`) in the root `esdiag` crate (where `tracing-subscriber` is a direct dependency; also promote `flate2.workspace = true` to `[dependencies]` for gzip compression) and expose it from `src/lib.rs`:
- The background batch worker takes `ElasticsearchClient` directly (built via `ElasticsearchBuilder` or `elasticsearch_client_from_output_host`), avoiding a single-implementation wrapper trait.
- Tests exercise the real `ElasticsearchClient` `_bulk` path against the existing `es_mock` server (`crates/esdiag-core/src/es_mock.rs`), which models `POST /{index}/_bulk` `create`-only data stream validation, `Authorization` headers, and scripted `BulkReply::Status(...)` failures.

### 2. Target resolution precedence and credential rules

`LogsTargetConfig::resolve()` evaluates in strict order:
1. `ESDIAG_LOGS_URL` present (non-empty):
   - Parse as `Url`.
   - Resolve auth exclusively from `ESDIAG_LOGS_APIKEY` or (`ESDIAG_LOGS_USERNAME` + `ESDIAG_LOGS_PASSWORD`) — mirroring `crates/esdiag-core/src/data/output_deployment.rs::environment_auth()`. Reject conflicting combinations (API key + basic auth, or only one of username/password).
   - Parse optional `ESDIAG_LOGS_INSECURE` (`true` / `1`, case-insensitive; default `false`).
   - Build `ElasticsearchClient` via `ElasticsearchBuilder::new(url).auth(auth).insecure(insecure)`.
2. `ESDIAG_LOGS_HOST` present (non-empty):
   - Resolve saved host from `KnownHost::get_known(&name)`.
   - Validate `host.app() == Some(Application::Elasticsearch)` and `host.has_role(HostRole::Send)`.
   - Build client via `elasticsearch_client_from_output_host(host)` (reuses `CredentialDirection::Output` and non-interactive keystore resolution via `ESDIAG_KEYSTORE_PASSWORD` or active unlock lease; never prompts interactively during tracing init).
   - Note: `logs.host` does **not** require a linked Kibana viewer (unlike `OutputDeployment`).
3. `ApplicationConfig::load()?.logs.host` present:
   - Same saved-host validation and non-interactive client construction as step 2.
4. Otherwise: `Ok(None)` (disabled).

Data stream resolution:
- `ESDIAG_LOGS_DATA_STREAM` if set and non-empty, else `"logs-esdiag-default"`.

Failure handling at startup:
- If `LogsTargetConfig::resolve()` returns `Err(err)` (e.g., invalid URL, unknown saved host, locked keystore, wrong host role), print a clear warning to `stderr` (`eprintln!("Remote logs exporter disabled: {err}")`) and continue with `None` so tracing initialization and command execution are never blocked.

### 3. `ApplicationConfig` schema extension

Add `LogsConfig` to `crates/esdiag-core/src/data/application_config.rs`:
```rust
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<u32>,
}
```
Attach `#[serde(default, skip_serializing_if = "LogsConfig::is_empty")] pub logs: LogsConfig` to `ApplicationConfig`.
- Keeps `CURRENT_VERSION = 1` (purely additive, backward-compatible with existing `esdiag.yml` files).
- Extend `ApplicationConfig::validate_references()` to validate `logs.host` when present: host exists in `hosts.yml`, `app() == Some(Application::Elasticsearch)`, `has_role(HostRole::Send)`.

### 4. Opt-in daily file logging (`ESDIAG_LOGS_PATH`), rollover, exit gzip, and retention

File logging directory resolution (`resolve_logs_dir()`):
1. If `ESDIAG_LOGS_PATH` is set in env:
   - Empty or whitespace-only → `None` (explicitly disables file logging even if `esdiag.yml` sets `logs.path`).
   - Non-empty → `Some(PathBuf::from(value))`.
2. Else if `ApplicationConfig::load().ok().and_then(|c| c.logs.path)` is `Some(path)`:
   - `Some(path)`.
3. Else → `None` (disabled by default).

Retention resolution (`resolve_logs_retention_days()`):
1. `ESDIAG_LOGS_RETENTION_DAYS` (parsed as positive `u32`; invalid/zero logs a warning to stderr and falls back to default).
2. Else `ApplicationConfig::load().ok().and_then(|c| c.logs.retention_days)`.
3. Else `30` days (`DEFAULT_LOGS_RETENTION_DAYS: u32 = 30`).

Provide a helper `default_logs_dir() -> Result<PathBuf>` returning `runtime_config_dir()?.join("logs")` (i.e. `~/.esdiag/logs`, respecting `ESDIAG_HOME`) for callers/config that want the default logs directory.

Daily rotating writer (`src/logging/file.rs::DailyLogWriter`):
- Implements `tracing_subscriber::fmt::MakeWriter<'_>` backed by a `Mutex<Inner>` holding `(current_date: NaiveDate, file: File)`, a `compress_on_rollover: bool` flag (`true` for `serve`, `false` for one-shot commands), and a clock closure (defaulting to `|| chrono::Utc::now().date_naive()`, injectable in tests).
- File naming: `esdiag-YYYY-MM-DD.log` inside the resolved logs directory, created with `0o600` permissions on Unix and opened in append mode.
- On each write / `make_writer()` call: compare `today` against `inner.current_date`. When `today != inner.current_date`:
  - Open `esdiag-{today}.log` and swap the active `File` handle immediately so writes are never stalled.
  - Spawn background maintenance (`tokio::task::spawn_blocking`) to prune expired files (older than `today - retention_days`) and, when `compress_on_rollover` is `true` (`serve` mode), gzip previous days' `.log` files.
- Startup performs **no** gzip compression and **no** directory scan, keeping startup fast and honoring the "no gzip on startup" requirement.
- Concurrent-safe compression helper (`compress_previous_logs(dir, today)`):
  - Scan `dir` for entries matching `esdiag-YYYY-MM-DD.log` where `date < today`.
  - Skip if `esdiag-YYYY-MM-DD.log.gz` already exists.
  - Stream-compress via `flate2::write::GzEncoder` to a unique sibling temp file (`esdiag-YYYY-MM-DD.log.gz.<pid>-<uuid>.tmp`) with `0o600` permissions on Unix, finish/sync, atomically rename to `esdiag-YYYY-MM-DD.log.gz`, then remove the uncompressed `.log` file (ignoring `ErrorKind::NotFound` if another concurrent process already compressed/removed it).
  - Never compress today's active `esdiag-{today}.log` file.
- On shutdown (`LogsGuard::shutdown()`):
  - Awaited inside `async_main()` before returning (so it completes before `main()`'s `runtime.shutdown_timeout(MCP_SIGNAL_SHUTDOWN_GRACE)` for `esdiag mcp`).
  - Flushes the remote Elasticsearch worker (bounded timeout) and, when file logging is enabled, flushes `DailyLogWriter` and runs `compress_previous_logs(dir, today)` + retention pruning (`esdiag-YYYY-MM-DD.log[.gz]` strictly older than `today - Duration::days(retention_days)`), ignoring non-matching files and reporting any I/O errors to stderr without failing process exit.
- Attach the file `fmt::layer()` to the tracing registry with `resolve_log_file_filter(&cli)` for **all** commands (including `serve`) whenever `resolve_logs_dir()` returns `Some(dir)`.

Impact on `src/local.rs`:
- `src/local.rs` currently calls `esdiag::data::last_run_path(RUN_LOG)` and `esdiag::data::open_run_log(false)` to capture `docker/podman compose` output.
- Update `local.rs` so `compose_logged` writes compose output to the active daily log file in `resolve_logs_dir()` when file logging is enabled, or captures stderr/stdout in memory (surfacing tail on failure) when file logging is disabled (`None`) — avoiding silently recreating `~/.esdiag/last_run/esdiag.log` when file logging is off.

### 5. Non-blocking layer, recursion guard, batching, and shutdown lifecycle

**State transitions for the exporter lifecycle:**
```
[Disabled]
   (valid target resolved)
[Active] ──(on_event: try_send)──> bounded mpsc channel (cap: 4096)
   │                                    │
   │                                    ├── full → drop event, increment atomic dropped counter
   │                                    └── background worker accumulates batch:
   │                                          flush when batch.len() >= 100 OR interval (2s) ticks
   │                                          POST /_bulk (create action -> data_stream)
   │                                          on HTTP/network error → eprintln! (rate-limited), drop batch
   │
   └──(LogsGuard::flush(timeout))──> [Draining]
                                        - drop sender / send shutdown signal
                                        - worker drains remaining channel items & sends final batch(es)
                                        - bounded by shutdown timeout (e.g., 3s)
                                     [Stopped]
```

**Recursion guard (critical):**
- When the background worker sends HTTP requests via `ElasticsearchClient` (which uses `reqwest`/`hyper`/`rustls`), those crates emit `tracing` events (e.g., `hyper::*`, `reqwest::*`, `h2::*`, `esdiag::logging::*`).
- Two-layer defense:
  1. Target filter on the Elasticsearch layer: ignore events whose target starts with `esdiag::logging`, `hyper`, `reqwest`, `h2`, `rustls`, `tower`, `elasticsearch` (or attach a thread-local / task-local re-entrancy guard `IN_LOG_EXPORTER` around `on_event` serialization and filter worker-originated HTTP transport targets).
  2. Never use `tracing::*` macros inside `src/logging/`; report internal errors via rate-limited `eprintln!`.

**Document schema:**
Each event is serialized to a `serde_json::Value` object:
```json
{
  "@timestamp": "2026-11-24T12:34:56.789Z",
  "log": {
    "level": "info",
    "logger": "esdiag::processor::elasticsearch"
  },
  "message": "processing started",
  "service": {
    "name": "esdiag",
    "version": "0.18.0-SNAPSHOT"
  },
  "esdiag": {
    "fields": { ... },
    "spans": [ { "name": "process", "fields": { ... } } ]
  }
}
```
- Top-level `@timestamp`, `log.level`, `log.logger`, `message`, `service.name`, `service.version` conform to ECS and work seamlessly with Elasticsearch's built-in `logs-*-*` data stream template.
- Event fields (other than `message`) are recorded via a `tracing::field::Visit` visitor under `esdiag.fields` (and span attributes captured in `on_new_span` / `on_record` via `tracing_subscriber::registry::Extensions` under `esdiag.spans`).

**Bulk request format:**
- Use `BulkOperation::create(doc)` (required for Elasticsearch data streams — `index` operations are rejected by data streams; `create` is mandatory).
- Do **not** specify `.pipeline("esdiag")` (the `esdiag` ingest pipeline is for diagnostic documents, not application logs).
- Apply a short per-request timeout (e.g., 5 seconds) so an unresponsive cluster never stalls the worker indefinitely.

## Risks / Trade-offs

- **Keystore unlock at startup when using `ESDIAG_LOGS_HOST` / `logs.host`**: Resolving a saved host with encrypted credentials requires either `ESDIAG_KEYSTORE_PASSWORD` or an active unlock lease. If neither is available, `LogsTargetConfig::resolve()` fails gracefully, warns on stderr, and continues without remote logging (never prompting interactively during tracing init).
- **Short-lived CLI commands and shutdown latency**: Flushing remaining logs on exit adds up to the HTTP round-trip time (capped at 3s timeout). When no remote logs target is configured, `LogsGuard::flush()` is an immediate no-op (0ms overhead).
- **Breaking change for `last_run/esdiag.log`**: Users or scripts expecting `~/.esdiag/last_run/esdiag.log` to exist by default will no longer see it unless `ESDIAG_LOGS_PATH` or `esdiag.yml` `logs.path` is set. Document this clearly in `CHANGELOG.md` and `docs/`.
- **Lite feature builds (`--no-default-features`)**: `src/logging/` must gate any keystore-only code appropriately (note: `KnownHost` and `ApplicationConfig` are always compiled in `crates/esdiag-core/src/data/`, and `elasticsearch_client_from_output_host` handles auth across feature flags — verify with `cargo check --no-default-features`).
