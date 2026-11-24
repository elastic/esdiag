## Purpose

Defines the transport-independent set of ESDiag tools offered to Model Context Protocol clients: their names, inputs, results, failures, safety annotations, and credential handling.

## ADDED Requirements

### Requirement: Tool Catalog
The MCP server SHALL offer the following tools, each performing the same operation as the named CLI command with the same defaults and validation:

| Tool | CLI command | Annotations |
|---|---|---|
| `host_list` | `esdiag host list` | read-only |
| `host_auth` | `esdiag host auth <target>` | read-only, open-world |
| `job_list` | `esdiag job list` | read-only |
| `output_show` | `esdiag output show` | read-only |
| `keystore_status` | `esdiag keystore status` | read-only |
| `job_run` | `esdiag job run <name>` | open-world |
| `process` | `esdiag process <input> [output]` | open-world |
| `collect` | `esdiag collect <host> <output>` | open-world |
| `agent_ask` | `esdiag agent ask <prompt>` | open-world |
| `send` | `esdiag send <file> <upload_id>` | open-world |
| `setup_output` | `esdiag setup output [name]` | open-world, idempotent |
| `setup_elasticsearch` | `esdiag setup elasticsearch [host]` | open-world, idempotent |
| `setup_kibana` | `esdiag setup kibana [host]` | open-world, idempotent |
| `host_add` | `esdiag host add` | none |
| `host_update` | `esdiag host update` | none |
| `output_set` | `esdiag output set <context>` | idempotent |
| `host_remove` | `esdiag host remove <name>` | destructive |
| `job_delete` | `esdiag job delete <name>` | destructive |
| `output_clear` | `esdiag output clear` | destructive, idempotent |

Each tool SHALL publish a JSON Schema for its parameters, with parameter names in snake_case matching the CLI option names. Tools whose annotation list omits read-only SHALL be marked as not read-only. The server SHALL identify itself as `esdiag` with the binary's version during MCP initialization instead of offering a version tool.

#### Scenario: Client reads the server version
- **WHEN** an MCP client initializes a session
- **THEN** the server information names `esdiag` and the same version `esdiag version` prints

#### Scenario: Client lists tools
- **GIVEN** an ESDiag binary built with default features
- **WHEN** an MCP client lists tools
- **THEN** the response contains exactly the tools in the catalog
- **AND** each tool has a description, an input schema, and the annotations listed for it

#### Scenario: Tool matches its CLI command
- **GIVEN** a saved host `prod` with role `collect`
- **WHEN** a client calls `host_auth` with `target: prod`
- **THEN** the result is the same typed outcome that `esdiag host auth prod --format json` emits

### Requirement: Tools Follow Cargo Features
A tool SHALL be offered only when the binary includes the cargo feature its CLI command requires: `job_run`, `job_list`, and `job_delete` require `keystore`; `agent_ask` requires `agent`; `setup_output`, `setup_elasticsearch`, and `setup_kibana` require `setup`.

#### Scenario: Build without agent support
- **GIVEN** an ESDiag binary built with `mcp` and without `agent`
- **WHEN** an MCP client lists tools
- **THEN** `agent_ask` is absent
- **AND** every tool whose required features are present is listed

### Requirement: Results Reuse The CLI Outcome Schema
A successful tool call SHALL return the command's typed outcome as structured content, identical to the JSON value `--format json` emits for the same command, and SHALL also return that JSON as a text content item for clients that ignore structured content. A tool SHALL NOT return an entire internal processor state or diagnostic report.

#### Scenario: Process returns the diagnostic identity
- **GIVEN** a diagnostic archive that processes successfully to a saved output host
- **WHEN** a client calls `process` with that archive as `input`
- **THEN** the structured content has `result: diagnostic_processed`
- **AND** it contains the primary `diagnostic.id`, the created document count, the duration in milliseconds, and the Kibana URL when available

#### Scenario: Job run returns every stage result
- **GIVEN** a saved job that collects, processes, and sends
- **WHEN** a client calls `job_run` with that job's name
- **THEN** the structured content identifies the job and includes the save, process, and send results

### Requirement: Failures Are Structured Tool Errors
An operation that fails after its parameters are accepted SHALL return a tool result marked as an error, whose structured content is the same structured failure the CLI emits, including its `category` and a message that contains no secret material. Arguments that do not match the tool's schema SHALL be rejected without running the operation, as a tool result marked as an error that has no structured content and whose text describes the mismatch, so that a model can correct its arguments.

#### Scenario: Unknown saved job
- **GIVEN** no saved job named `nightly`
- **WHEN** a client calls `job_run` with `name: nightly`
- **THEN** the tool result is marked as an error
- **AND** its structured content has `category: not_found` naming `nightly`

#### Scenario: Schema violation
- **WHEN** a client calls `collect` without the required `host` parameter
- **THEN** the tool result is marked as an error without structured content
- **AND** no collection starts

#### Scenario: Unknown parameter
- **WHEN** a client calls `process` with a `sources` parameter
- **THEN** the tool result is marked as an error without structured content
- **AND** no processing starts

### Requirement: Tools Never Accept Raw Credentials
Tool parameters SHALL NOT accept passwords, API keys, keystore passwords, or Elastic Upload Service download tokens. A URL or URL template parameter that embeds userinfo SHALL be rejected with `category: invalid_input` before the operation runs, and the result SHALL NOT repeat the credential. An Elastic Upload Service upload ID is not a credential. `host_add` and `host_update` SHALL accept authentication only as a `secret` reference to an existing keystore entry. Host results SHALL omit userinfo from saved URLs and URL templates. A tool that needs the keystore while it is locked SHALL fail with `category: keystore_failed` and a message directing the user to unlock it from a terminal, and SHALL NOT prompt for input.

#### Scenario: Host add schema has no secret fields
- **WHEN** a client lists tools
- **THEN** the `host_add` and `host_update` input schemas contain a `secret` parameter
- **AND** contain no `apikey`, `username`, or `password` parameter

#### Scenario: Host target with embedded credentials
- **WHEN** a client calls `host_add` with a target or URL template such as `https://elastic:secret@example.test:9200`
- **THEN** the tool result is an error with `category: invalid_input`
- **AND** the result does not contain the password
- **AND** no host is saved

#### Scenario: Authenticated download link
- **WHEN** a client calls `process` with an Elastic Upload Service download URL that carries its token
- **THEN** the tool result is an error with `category: invalid_input`
- **AND** no processing starts

#### Scenario: Saved URL credentials are not listed
- **GIVEN** a saved host whose URL embeds a password
- **WHEN** a client calls `host_list`
- **THEN** the host's target omits the URL userinfo

#### Scenario: Locked keystore
- **GIVEN** a saved host that authenticates through a keystore secret
- **AND** the keystore is locked and `ESDIAG_KEYSTORE_PASSWORD` is unset
- **WHEN** a client calls `collect` for that host
- **THEN** the tool result is an error with `category: keystore_failed`
- **AND** the server does not read from its standard input

#### Scenario: Locked keystore in a saved job
- **GIVEN** a saved job that collects from a host that authenticates through a keystore secret
- **AND** the keystore is locked and `ESDIAG_KEYSTORE_PASSWORD` is unset
- **WHEN** a client calls `job_run` for that job
- **THEN** the tool result is an error with `category: keystore_failed` and the unlock guidance

### Requirement: Tools Never Write Diagnostic Data To The Protocol Stream
`process` SHALL reject `-` as its `output` with `category: invalid_input`. `job_run` SHALL reject a saved job that exports processed documents to stdout with `category: invalid_input`, before any stage runs. No tool SHALL accept the `sources` override, because the override applies to the whole process rather than one call.

#### Scenario: Stdout output rejected
- **WHEN** a client calls `process` with `output: "-"`
- **THEN** the tool result is an error with `category: invalid_input`
- **AND** no document is written

#### Scenario: Saved job exporting to stdout rejected
- **GIVEN** a saved job `stream` whose processing output is `-`
- **WHEN** a client calls `job_run` with `name: stream`
- **THEN** the tool result is an error with `category: invalid_input`
- **AND** no collection or processing starts

#### Scenario: Sources parameter absent
- **WHEN** a client lists tools
- **THEN** neither the `process` nor the `collect` input schema contains a `sources` parameter

### Requirement: Long Operations Report Progress
When the client supplies a progress token, `collect`, `process`, `job_run`, and `send` SHALL emit MCP progress notifications as each execution stage starts, progresses, and completes, with a message naming the stage. When a client cancels a call, the server SHALL stop the operation, SHALL NOT report it as successful, and SHALL continue serving other requests.

#### Scenario: Processing progress
- **GIVEN** a client calls `process` with a progress token
- **WHEN** the diagnostic is processed
- **THEN** the client receives at least one progress notification before the result
- **AND** each notification's message names the current stage

#### Scenario: Cancelled collection
- **GIVEN** a `collect` call is in progress
- **WHEN** the client sends a cancellation for that request
- **THEN** no success result is returned for the request
- **AND** a subsequent `host_list` call succeeds

### Requirement: Local Configuration Writes Are Serialized
Tools that change hosts, saved jobs, or the output context SHALL run one at a time, so that concurrent calls cannot lose each other's changes. A `collect` or `process` call with `save_job` counts as such a tool for its whole duration. Read-only tools and other long-running operations MAY run concurrently.

#### Scenario: Concurrent host additions
- **GIVEN** no saved hosts named `a` or `b`
- **WHEN** a client calls `host_add` for `a` and `host_add` for `b` concurrently
- **THEN** both calls succeed
- **AND** a following `host_list` contains both `a` and `b`
