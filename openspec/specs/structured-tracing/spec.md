# Structured Tracing

## Purpose

Defines how the application initializes and uses structured tracing via the `tracing` crate ecosystem, replacing ad-hoc logging with span-aware, level-filtered, and environment-configurable observability.

## Requirements

### Requirement: Tracing subscriber initializes with environment-driven log level
The system SHALL initialize a `tracing-subscriber` registry on startup that always attaches a stderr fmt layer, optionally attaches a file fmt layer when file logging is enabled via `ESDIAG_LOGS_PATH` or `esdiag.yml` `logs.path`, and optionally attaches the remote Elasticsearch logs exporter layer when a remote logs target is configured. Each layer SHALL respect the `LOG_LEVEL` environment variable (defaulting to `info`) and override to `debug` level when the `--debug` CLI flag is set (with terminal stderr keeping its existing `warn` default for non-server finite commands and agent mode).

#### Scenario: Default log level from environment
- **GIVEN** `LOG_LEVEL` is unset and `--debug` is not passed
- **WHEN** the application starts in server mode or with file/remote log layers enabled
- **THEN** the tracing subscriber filters at `info` level for those sinks

#### Scenario: Debug flag overrides level
- **GIVEN** the application starts with `--debug`
- **WHEN** tracing initializes
- **THEN** all active tracing layers filter at `debug` level regardless of `LOG_LEVEL`

#### Scenario: LOG_LEVEL env var is respected
- **GIVEN** `LOG_LEVEL=warn` and `--debug` is not passed
- **WHEN** the application starts
- **THEN** active tracing layers filter at `warn` level

#### Scenario: File layer is omitted when file logging is not configured
- **GIVEN** `ESDIAG_LOGS_PATH` is unset and `esdiag.yml` has no `logs.path`
- **WHEN** tracing initializes
- **THEN** no file logging layer is attached and `~/.esdiag/last_run/esdiag.log` is not created

#### Scenario: Remote Elasticsearch layer is attached alongside stderr
- **GIVEN** a valid remote logs target is configured via `ESDIAG_LOGS_URL` or `ESDIAG_LOGS_HOST` or `esdiag.yml` `logs.host`
- **WHEN** tracing initializes
- **THEN** the remote Elasticsearch logs layer is attached alongside the stderr layer
- **AND** tracing events are delivered to both stderr (per terminal filter) and the remote Elasticsearch target

### Requirement: log crate events are forwarded through tracing
The system SHALL install a `tracing-log` compatibility layer so that events emitted by third-party dependencies using the `log` crate are captured by the tracing subscriber.

#### Scenario: Dependency log events are visible
- **WHEN** a dependency emits a `log::warn!` event
- **THEN** that event appears in tracing output at the equivalent level

### Requirement: Async entry points are instrumented with spans
The system SHALL annotate key async entry points — including the top-level `run()` function, `Processor::process()`, `Receiver::get()` and `Receiver::get_stream()`, and `Exporter::send()` and `Exporter::document_channel()` — with `#[tracing::instrument]` to provide span-based context in log output.

#### Scenario: Span appears in output for processor entry
- **WHEN** a processor's entry point is invoked
- **THEN** a tracing span named after that function is active for the duration of the call

#### Scenario: Nested span context propagates through await points
- **WHEN** an instrumented async function awaits another instrumented function
- **THEN** the parent span remains the active context in the child span's output
