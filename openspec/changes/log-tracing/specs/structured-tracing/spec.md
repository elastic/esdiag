## ADDED Requirements

### Requirement: Tracing subscriber initializes with environment-driven log level
The system SHALL initialize a `tracing-subscriber` fmt subscriber on startup, respecting the `LOG_LEVEL` environment variable (defaulting to `info`) and overriding to `debug` level when the `--debug` CLI flag is set.

#### Scenario: Default log level from environment
- **WHEN** the application starts without `--debug` and `LOG_LEVEL` is unset
- **THEN** the tracing subscriber filters at `info` level

#### Scenario: Debug flag overrides level
- **WHEN** the application starts with `--debug`
- **THEN** the tracing subscriber filters at `debug` level regardless of `LOG_LEVEL`

#### Scenario: LOG_LEVEL env var is respected
- **WHEN** the application starts with `LOG_LEVEL=warn` and without `--debug`
- **THEN** the tracing subscriber filters at `warn` level

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
