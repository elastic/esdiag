## MODIFIED Requirements

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
