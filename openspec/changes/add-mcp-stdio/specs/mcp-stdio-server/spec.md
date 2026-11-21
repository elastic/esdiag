## Purpose

Defines the `esdiag mcp` subcommand that serves the ESDiag MCP tool catalog to a local client over standard input and output, and the cargo feature that builds it.

## ADDED Requirements

### Requirement: MCP Cargo Feature
The `esdiag` package SHALL provide an `mcp` cargo feature that builds the MCP tool catalog and the `esdiag mcp` subcommand. The `full` feature, and therefore the default build, SHALL include `mcp`. A build without `mcp` SHALL NOT offer the `mcp` subcommand and SHALL NOT compile the MCP SDK.

#### Scenario: Default build offers the subcommand
- **GIVEN** an ESDiag binary built with default features
- **WHEN** the user runs `esdiag --help`
- **THEN** the command list includes `mcp`

#### Scenario: Build without the feature
- **GIVEN** an ESDiag binary built with default features disabled and `mcp` not enabled
- **WHEN** the user runs `esdiag mcp`
- **THEN** argument parsing rejects `mcp` as an unknown subcommand

#### Scenario: Feature builds alone
- **GIVEN** the root package
- **WHEN** a maintainer builds it with default features disabled and only `mcp` enabled
- **THEN** the build succeeds
- **AND** the tool catalog offers only the tools whose required features are present

### Requirement: Stdio Transport
`esdiag mcp` SHALL run an MCP server that reads JSON-RPC messages from standard input and writes them to standard output, using the newline-delimited stdio transport defined by the MCP specification. The server SHALL exit with status zero when its standard input reaches end of file, and SHALL stop accepting new requests when it receives an interrupt or termination signal.

#### Scenario: Client handshake
- **GIVEN** a client starts `esdiag mcp` as a child process
- **WHEN** the client sends an `initialize` request followed by the `initialized` notification
- **THEN** the server responds with its capabilities, including tools
- **AND** a following `tools/list` request returns the tool catalog

#### Scenario: Client disconnects
- **GIVEN** a running `esdiag mcp` session with no request in progress
- **WHEN** the client closes the server's standard input
- **THEN** the process exits with status zero

### Requirement: Standard Output Carries Only Protocol Messages
While `esdiag mcp` runs, standard output SHALL contain only MCP protocol messages. All tracing, warnings, and progress text SHALL go to standard error or the configured log file, at the agent-mode default level unless `--debug` or `RUST_LOG` selects another. The global `--format` option SHALL have no effect on protocol messages.

#### Scenario: Debug logging does not corrupt the stream
- **GIVEN** a client runs `esdiag --debug mcp`
- **WHEN** the client calls `process` on a diagnostic archive
- **THEN** every line on standard output parses as one JSON-RPC message
- **AND** debug log records appear on standard error

### Requirement: Server Never Prompts
`esdiag mcp` SHALL NOT read interactive input from the terminal, including keystore passwords, confirmations, and onboarding answers, even when standard error is a terminal. The keystore SHALL be usable only through `ESDIAG_KEYSTORE_PASSWORD` or an unlock lease created before the call.

#### Scenario: Unlock lease created from a terminal
- **GIVEN** the user unlocked the keystore from a terminal and the unlock lease is active
- **WHEN** a client calls `job_run` for a saved job whose host uses a keystore secret
- **THEN** the job authenticates with that secret
- **AND** the server does not prompt
