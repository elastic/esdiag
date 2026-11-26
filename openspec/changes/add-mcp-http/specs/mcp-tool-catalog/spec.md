## MODIFIED Requirements

### Requirement: Tool Catalog
`esdiag mcp`, and the `/mcp` HTTP endpoint in `user` mode, SHALL offer the following tools, each performing the same operation as the named CLI command with the same defaults and validation. The `/mcp` endpoint in `service` mode SHALL NOT offer this catalog; it offers only the restricted `process` tool that `mcp-http-server` defines, which does not match a CLI command's parameters or defaults.

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
- **GIVEN** `esdiag mcp` or `/mcp` in either mode
- **WHEN** an MCP client initializes a session
- **THEN** the server information names `esdiag` and the same version `esdiag version` prints

#### Scenario: Client lists tools
- **GIVEN** an ESDiag binary built with default features
- **WHEN** an MCP client lists tools from `esdiag mcp` or from `/mcp` in `user` mode
- **THEN** the response contains exactly the tools in the catalog
- **AND** each tool has a description, an input schema, and the annotations listed for it

#### Scenario: Service mode does not offer the catalog
- **GIVEN** `esdiag serve` runs in `service` mode with the `mcp` web feature
- **WHEN** an MCP client lists tools from `/mcp`
- **THEN** the response contains only the restricted `process` tool defined by `mcp-http-server`

#### Scenario: Tool matches its CLI command
- **GIVEN** a saved host `prod` with role `collect`
- **WHEN** a client calls `host_auth` with `target: prod`
- **THEN** the result is the same typed outcome that `esdiag host auth prod --format json` emits

### Requirement: Tools Never Accept Raw Credentials
Tool parameters SHALL NOT accept passwords, API keys, keystore passwords, or Elastic Upload Service download tokens. The one exception is the `token` parameter of the `service`-mode `process` tool that `mcp-http-server` defines, which accepts only a download token, separate from its URL, because download tokens are short-lived and scoped to one upload. A URL or URL template parameter that embeds userinfo SHALL be rejected with `category: invalid_input` before the operation runs, and the result SHALL NOT repeat the credential. An Elastic Upload Service upload ID is not a credential. `host_add` and `host_update` SHALL accept authentication only as a `secret` reference to an existing keystore entry. Host results SHALL omit userinfo from saved URLs and URL templates. A tool that needs the keystore while it is locked SHALL fail with `category: keystore_failed` and a message directing the user to unlock it from a terminal, and SHALL NOT prompt for input.

#### Scenario: Host add schema has no secret fields
- **GIVEN** `esdiag mcp` or `/mcp` in `user` mode
- **WHEN** a client lists tools
- **THEN** the `host_add` and `host_update` input schemas contain a `secret` parameter
- **AND** contain no `apikey`, `username`, or `password` parameter

#### Scenario: Host target with embedded credentials
- **GIVEN** `esdiag mcp` or `/mcp` in `user` mode
- **WHEN** a client calls `host_add` with a target or URL template such as `https://elastic:secret@example.test:9200`
- **THEN** the tool result is an error with `category: invalid_input`
- **AND** the result does not contain the password
- **AND** no host is saved

#### Scenario: Authenticated download link
- **GIVEN** `esdiag mcp` or `/mcp` in `user` mode
- **WHEN** a client calls `process` with an Elastic Upload Service download URL that carries its token
- **THEN** the tool result is an error with `category: invalid_input`
- **AND** no processing starts

#### Scenario: Local catalog offers no token parameter
- **GIVEN** `esdiag mcp` or `/mcp` in `user` mode
- **WHEN** a client lists tools
- **THEN** no tool input schema contains a `token` parameter

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

