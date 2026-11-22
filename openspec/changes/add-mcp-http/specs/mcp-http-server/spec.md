## Purpose

Defines the Streamable HTTP MCP endpoint that `esdiag serve` exposes at `/mcp`: when it is mounted, which tools each runtime mode offers, and how requests are authenticated and validated.

## ADDED Requirements

### Requirement: MCP Endpoint Enablement
The web server SHALL mount a Model Context Protocol endpoint at `/mcp`, using the Streamable HTTP transport, only when the binary includes MCP support, server policy allows the `mcp` web feature, and the server was started by `esdiag serve`. The desktop app's server SHALL NOT mount `/mcp` for any web feature list or build. When the endpoint is not mounted, requests to `/mcp` SHALL receive the router's not-found response.

#### Scenario: Endpoint enabled
- **GIVEN** an ESDiag binary built with default features
- **AND** the server starts with `--web-features advanced,mcp`
- **WHEN** an MCP client sends an `initialize` request to `/mcp`
- **THEN** the server responds with its capabilities, including tools

#### Scenario: Endpoint disabled by default
- **GIVEN** the server starts in either runtime mode without `mcp` in its web features
- **WHEN** a client posts to `/mcp`
- **THEN** the response is not found

#### Scenario: Desktop ignores an MCP feature override
- **GIVEN** a binary built with default features
- **AND** `ESDIAG_WEB_FEATURES=advanced,mcp` is set
- **WHEN** the desktop app starts its server and a client posts to `/mcp`
- **THEN** the response is not found
- **AND** the desktop server starts with a warning that the MCP endpoint is not available

### Requirement: Stateless Transport
The endpoint SHALL serve each request without server-held MCP session state, so that any server instance can answer any request. Responses that carry progress notifications SHALL be streamed as server-sent events. Simple responses SHALL be returned as JSON. When a client closes the response stream of an in-progress call, or the server shuts down, the server SHALL stop that operation, including any Elastic Upload Service download and queued exports, and SHALL NOT report it as successful. A stopped call's concurrency slot SHALL be released only after its work has stopped.

#### Scenario: Client disconnects during a download
- **GIVEN** the server runs in `service` mode with a per-owner cap of 1
- **AND** an owner's `process` call is waiting on an Elastic Upload Service download that has not finished
- **WHEN** the client closes the response stream
- **THEN** the server closes the download connection
- **AND** the call is not reported as successful
- **AND** the owner's next `process` call is admitted

#### Scenario: Client disconnects during an export
- **GIVEN** the server runs in `service` mode with a per-owner cap of 1
- **AND** an owner's `process` call is exporting a diagnostic in several bulk requests, and one request is in flight
- **WHEN** the client closes the response stream
- **THEN** the server sends no further bulk request for that call
- **AND** the owner's next `process` call is rejected until the in-flight request ends, and admitted afterwards

#### Scenario: Requests served without a session
- **GIVEN** the endpoint is enabled
- **WHEN** a client sends `tools/list` without an MCP session identifier
- **THEN** the server returns the tool list

#### Scenario: Progress streamed for a long call
- **GIVEN** the endpoint is enabled in `user` mode
- **WHEN** a client calls `process` with a progress token
- **THEN** the response is an event stream containing at least one progress notification followed by the result

### Requirement: User Mode Tool Policy
In `user` mode, the endpoint SHALL offer the same tools, parameters, results, failures, and annotations as `esdiag mcp` for the same build.

#### Scenario: Same catalog as stdio
- **GIVEN** a binary built with default features
- **WHEN** a client lists tools from `/mcp` in `user` mode and from `esdiag mcp`
- **THEN** both lists contain the same tool names and input schemas

### Requirement: Service Mode Tool Policy
In `service` mode, the endpoint SHALL offer only `process`, with two required parameters: `url` and `token`. `url` SHALL accept only an absolute `https` URL on `upload.elastic.co` with no userinfo, query, or fragment. Archive paths, directories, saved host names, other URLs, and URLs that embed a token SHALL be rejected with `category: invalid_input`. Validation SHALL be syntactic: it SHALL NOT resolve saved hosts, read the keystore, or inspect the filesystem, so rejected input causes no local-state access. `token` SHALL be the Elastic Upload Service download token for that URL. The token SHALL NOT appear in tool results, failures, or logs, and SHALL NOT be persisted. The schema SHALL NOT offer `output` or `save_job`; processed documents SHALL go to the startup-defined exporter. No tool in `service` mode SHALL read or write `hosts.yml`, `jobs.yml`, the keystore, or the output context. This restriction SHALL hold for every authentication provider.

#### Scenario: Service mode lists one tool
- **GIVEN** the server runs in `service` mode with the `mcp` web feature
- **WHEN** a client lists tools
- **THEN** the list contains only `process`
- **AND** its input schema contains `url` and `token` and no `output` or `save_job` parameter

#### Scenario: Upload service input processed to the shared cluster
- **GIVEN** the server runs in `service` mode with a startup-defined Elasticsearch exporter
- **WHEN** a client calls `process` with an Elastic Upload Service URL for a valid diagnostic and its download token
- **THEN** the diagnostic is processed to the startup-defined exporter
- **AND** the result has `result: diagnostic_processed`
- **AND** the result does not contain the token

#### Scenario: Local path rejected
- **GIVEN** the server runs in `service` mode
- **WHEN** a client calls `process` with `url: /tmp/diag.zip`
- **THEN** the tool result is an error with `category: invalid_input`
- **AND** the server does not read the path

#### Scenario: Saved host name rejected without local state
- **GIVEN** the server runs in `service` mode
- **AND** the server's user has a saved host named `prod`
- **WHEN** a client calls `process` with `url: prod`
- **THEN** the tool result is an error with `category: invalid_input`
- **AND** the server does not read `hosts.yml` or the keystore

#### Scenario: Embedded token rejected
- **GIVEN** the server runs in `service` mode
- **WHEN** a client calls `process` with a `url` of the form `https://token:<token>@upload.elastic.co/...`
- **THEN** the tool result is an error with `category: invalid_input`
- **AND** the error does not repeat the token

### Requirement: Service Mode Concurrency Caps Apply
In `service` mode, each `process` call SHALL count against the per-owner and global concurrent-job caps, with the owner resolved from the request's authenticated identity. A call that would exceed either cap SHALL return a tool error stating which cap was reached, and SHALL NOT start processing.

#### Scenario: Owner at the cap
- **GIVEN** the server runs in `service` mode with a per-owner cap of 1
- **AND** owner `operator@example.com` has one active job
- **WHEN** that owner calls `process`
- **THEN** the tool result is an error naming the per-owner cap
- **AND** no additional job starts

### Requirement: Requests Use The Configured Authentication Provider
Requests to `/mcp` SHALL be authenticated by the server's configured request authentication provider, exactly as other web requests are. Requests that fail authentication SHALL be rejected with an HTTP error before any MCP message is processed. Every tool call that runs a job SHALL resolve the request identity the same way web jobs do, for every provider including `none`, and SHALL record that user and account in the diagnostic's `Identifiers` and use that user as the job's admission owner. In `service` mode, identity resolution SHALL NOT read user configuration files.

#### Scenario: Identity-aware proxy header required
- **GIVEN** the server runs in `service` mode with the Google identity-aware proxy provider and the `mcp` web feature
- **WHEN** a client posts to `/mcp` without the identity header
- **THEN** the server rejects the request with the same status other authenticated routes return
- **AND** no tool runs

#### Scenario: Identity recorded on the processed diagnostic
- **GIVEN** a request authenticated as `operator@example.com`
- **WHEN** the request's `process` call completes
- **THEN** the processed diagnostic's report identifies `operator@example.com` as its user

#### Scenario: Configured identity used without an authentication provider
- **GIVEN** the server runs in `service` mode with authentication provider `none`
- **AND** `ESDIAG_USER=operator@example.com` and `ESDIAG_ACCOUNT=acme` are set
- **WHEN** a client's `process` call completes
- **THEN** the processed diagnostic's report identifies `operator@example.com` as its user and `acme` as its account
- **AND** the job counted against the owner `operator@example.com` while it ran

### Requirement: Host And Origin Validation
The endpoint SHALL reject requests whose `Host` header is not an allowed host, and requests that carry an `Origin` header whose scheme, host, and port do not match an allowed host, with HTTP status 403. By default the allowed hosts SHALL be `localhost`, `127.0.0.1`, and `::1`. `serve --mcp-allowed-hosts`, then `ESDIAG_MCP_ALLOWED_HOSTS`, SHALL replace the default with a comma-separated list of host names or `host:port` authorities.

#### Scenario: DNS rebinding rejected
- **GIVEN** the endpoint is enabled with default allowed hosts
- **WHEN** a request arrives with `Host: attacker.example`
- **THEN** the server responds with 403
- **AND** no tool runs

#### Scenario: Public deployment hostname allowed
- **GIVEN** the server starts with `ESDIAG_MCP_ALLOWED_HOSTS=esdiag.example.com`
- **WHEN** an authenticated request arrives with `Host: esdiag.example.com`
- **THEN** the server processes the MCP request

#### Scenario: Cross-site origin rejected
- **GIVEN** the endpoint is enabled with default allowed hosts
- **WHEN** a request arrives with `Host: 127.0.0.1:2501` and `Origin: https://evil.example`
- **THEN** the server responds with 403
