## Context

ESDiag currently uses the `log` crate (v0.4) for logging across all modules, with `env_logger` as the backend, initialized in `main.rs`. The project is built on Tokio and uses async throughout. The `log` crate is unstructured (text-only), has no concept of async spans, and offers no context propagation across await points — limiting the usefulness of log output when diagnosing async pipeline behavior.

The `tracing` crate is the de-facto standard for async Rust instrumentation. It provides span-based context, structured fields, and drop-in macro compatibility with `log`. The `tracing-log` bridge means third-party crates using `log` will still emit events through `tracing-subscriber`.

## Goals / Non-Goals

**Goals:**
- Replace all `log::` macro call sites with `tracing::` equivalents
- Replace `env_logger` initialization with `tracing-subscriber` (maintaining `LOG_LEVEL` env var and `--debug` flag behaviour)
- Add `#[tracing::instrument]` to key async processor, receiver, and exporter entry points
- Wire `tracing-log` so crates that still use `log` emit through the tracing subscriber

**Non-Goals:**
- Adding distributed tracing (OpenTelemetry, Jaeger) — the infrastructure for exporting traces is out of scope
- Changing log level semantics or the logging output format beyond what `tracing-subscriber`'s default fmt subscriber provides
- Instrumenting every function — only meaningful async entry points get `#[instrument]`

## Decisions

### 1. `tracing-subscriber` with `EnvFilter` instead of `env_logger`

**Decision**: Use `tracing_subscriber::fmt()` with `EnvFilter` built from the `LOG_LEVEL` env var.

**Rationale**: Direct equivalent of `env_logger::Env::default().filter_or(...)`. `EnvFilter` supports the same `RUST_LOG`-style directives, so existing user muscle-memory and CI env var config stays valid.

**Alternative considered**: `tracing-bunyan-formatter` for JSON output — rejected because it changes the human-readable format that users currently see; JSON output can be added later as an opt-in flag.

### 2. Keep macro call sites simple — no structured fields in this pass

**Decision**: Replace `log::info!(...)` with `tracing::info!(...)` mechanically. Do not add structured fields (e.g. `host = %h`) to existing call sites in this change.

**Rationale**: Adding structured fields at existing call sites is a separate concern and risks scope creep. The macro signatures are compatible, so mechanical replacement is low-risk and immediately testable. Structured field adoption can follow incrementally.

**Alternative considered**: Annotating fields at every call site in one pass — rejected as too large a diff to review safely.

### 3. `#[tracing::instrument]` on async entry points only

**Decision**: Annotate the main async `run()` function and the top-level `process()` / `receive()` / `export()` trait method impls.

**Rationale**: These are the call sites where async context is most valuable for diagnosing failures. Instrumenting every function would generate noisy spans.

### 4. `tracing-log` compatibility bridge

**Decision**: Enable the `tracing-log` feature on `tracing-subscriber` (or add `tracing-log` explicitly) so third-party `log` crate users emit through the tracing pipeline.

**Rationale**: Several dependencies (e.g. `reqwest`, `hyper`) still emit via `log`. Without the bridge, those events would disappear when `env_logger` is removed.

## Risks / Trade-offs

- **Timestamp format change** → `tracing-subscriber`'s default fmt differs slightly from `env_logger`'s `format_timestamp_millis()`. Mitigation: use `.with_timer(tracing_subscriber::fmt::time::uptime())` or configure a matching format.
- **Missed call sites** → 61 files with `log::` usage; any missed replacement will cause a compile error (no `log` dep), which is a hard safety net. Risk is low.
- **Span overhead in tight loops** → Processor inner loops creating spans on every iteration would regress performance. Mitigation: `#[instrument]` is applied only at function granularity on entry points, not inside loops.

## Migration Plan

1. Add `tracing`, `tracing-subscriber` (with `env-filter` + `fmt` features), `tracing-log` to `Cargo.toml`
2. Replace `env_logger` init block in `main.rs` with `tracing_subscriber::fmt()` setup
3. Do a codebase-wide mechanical replacement of `use log` → `use tracing` and `log::X!` → `tracing::X!`
4. Add `#[tracing::instrument]` to targeted async entry points
5. Remove `log` and `env_logger` from `Cargo.toml`
6. Run `cargo build` and fix any remaining compile errors
7. Verify log output format in both `--debug` and default modes

**Rollback**: Revert Cargo.toml and the macro replacements — no schema, API, or persistent state changes are involved.

## Open Questions

~~- Should timestamp formatting match the existing millis format exactly, or is a minor format change acceptable?~~
**Resolved**: Minor format change is acceptable — use `tracing-subscriber`'s default timestamp format.

~~- Are there any integration tests or snapshot tests that assert on log output format that would need updating?~~
**Resolved**: No log format compatibility concerns. If any tests fail due to format changes, refactor them.
