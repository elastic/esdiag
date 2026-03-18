## Why

The project uses the `log` crate with `env_logger` for logging, which provides only unstructured text output and has no awareness of async context. Replacing this with the `tracing` crate gives span-based structured instrumentation that integrates natively with Tokio, enables richer diagnostics, and aligns with the Rust async ecosystem standard.

## What Changes

- Replace `log` crate dependency with `tracing`
- Replace `env_logger` with `tracing-subscriber` (with `EnvFilter` for `LOG_LEVEL` / `--debug` parity)
- Replace all `log::debug!`, `log::info!`, `log::warn!`, `log::error!`, `log::trace!` call sites (61 files) with the equivalent `tracing::` macros
- Add `#[tracing::instrument]` annotations to key async entry points in processors, receivers, and exporters
- Remove `log` and `env_logger` from `Cargo.toml`; add `tracing` and `tracing-subscriber`

## Capabilities

### New Capabilities
- `structured-tracing`: Span-based structured logging and instrumentation using the `tracing` crate, providing async-aware context propagation and structured field support across CLI and core processing logic.

### Modified Capabilities

## Impact

- **Core**: All 61 Rust source files using `log::` macros — `src/main.rs`, `src/setup.rs`, `src/processor/**`, `src/receiver/**`, `src/exporter/**`, `src/server/**`, `src/client/**`
- **Dependencies**: `Cargo.toml` — remove `log`, `env_logger`; add `tracing`, `tracing-subscriber`
- **CLI**: Log initialization in `src/main.rs` — `env_logger::Builder` replaced with `tracing_subscriber` setup
- **No API or web UI changes** — internal instrumentation only
