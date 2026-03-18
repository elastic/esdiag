## 1. Dependency Setup

- [x] 1.1 Add `tracing` and `tracing-subscriber` (features: `env-filter`, `fmt`) to `Cargo.toml`
- [x] 1.2 Add `tracing-log` to `Cargo.toml` for backward-compatible log bridge
- [x] 1.3 Remove `log` and `env_logger` from `Cargo.toml`

## 2. Subscriber Initialization

- [x] 2.1 Replace `env_logger::Builder` init block in `src/main.rs` with `tracing_subscriber::fmt()` setup using `EnvFilter` (respecting `LOG_LEVEL` env var defaulting to `info`)
- [x] 2.2 Replace the `--debug` branch to use `EnvFilter::new("debug")` instead of `log::LevelFilter::Debug`
- [x] 2.3 Add `tracing-log` as a direct dependency and enable its feature on `tracing-subscriber`; call `tracing_log::LogTracer::init().ok()` in `src/main.rs` before subscriber init so third-party `log` crate events are forwarded through the tracing pipeline

## 3. Macro Call Site Replacement

- [x] 3.1 Replace `use log` imports with `use tracing` across all source files
- [x] 3.2 Replace `log::debug!` → `tracing::debug!` across all 61 affected files
- [x] 3.3 Replace `log::info!` → `tracing::info!` across all affected files
- [x] 3.4 Replace `log::warn!` → `tracing::warn!` across all affected files
- [x] 3.5 Replace `log::error!` → `tracing::error!` across all affected files
- [x] 3.6 Replace `log::trace!` → `tracing::trace!` across all affected files
- [x] 3.7 Replace `log::LevelFilter` references (e.g. in `src/main.rs`) with `tracing_subscriber` equivalents

## 4. Span Instrumentation

- [x] 4.1 Add `#[tracing::instrument]` to the top-level `run()` async function in `src/main.rs`
- [x] 4.2 Add `#[tracing::instrument(skip(self))]` to the primary `receive()` entry point implementations in `src/receiver/`
- [x] 4.3 Add `#[tracing::instrument(skip(self))]` to the primary `process()` entry point implementations in `src/processor/`
- [x] 4.4 Add `#[tracing::instrument(skip(self))]` to the primary `export()` entry point implementations in `src/exporter/`

## 5. Verification

- [x] 5.1 Run `cargo build` and resolve any remaining compile errors from missing `log` imports
- [x] 5.2 Run `cargo clippy` and fix any warnings
- [x] 5.3 Run `cargo test` and ensure all tests pass
- [x] 5.4 Manually verify `--debug` flag produces debug-level output
- [x] 5.5 Manually verify `LOG_LEVEL=warn` suppresses info output

## 6. Performance Regression Check

- [x] 6.1 Before implementing: time `esdiag process` against each archive in `tests/archives/` writing to a temp output directory; record wall-clock times as a baseline
- [x] 6.2 After implementing: repeat the same timed runs against all six archives (`elasticsearch-api-diagnostics-7.17.29.zip`, `8.19.3.zip`, `9.1.3.zip`, `kibana-api-diagnostics-7.17.29.zip`, `8.19.3.zip`, `9.1.3.zip`)
- [x] 6.3 Confirm each run is within 5% of its baseline; if any run exceeds 5%, profile and fix before merging
