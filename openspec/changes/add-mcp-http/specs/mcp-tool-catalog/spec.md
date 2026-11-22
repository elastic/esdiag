## MODIFIED Requirements

### Requirement: Tools Never Accept Raw Credentials
Tool parameters SHALL NOT accept passwords, API keys, keystore passwords, or Elastic Upload Service download tokens. The one exception is the `token` parameter of the `service`-mode `process` tool that `mcp-http-server` defines, which accepts only a download token, separate from its URL. A URL or URL template parameter that embeds userinfo SHALL be rejected with `category: invalid_input` before the operation runs, and the result SHALL NOT repeat the credential. An Elastic Upload Service upload ID is not a credential. `host_add` and `host_update` SHALL accept authentication only as a `secret` reference to an existing keystore entry. Host results SHALL omit userinfo from saved URLs and URL templates. A tool that needs the keystore while it is locked SHALL fail with `category: keystore_failed` and a message directing the user to unlock it from a terminal, and SHALL NOT prompt for input.

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

#### Scenario: Local catalog offers no token parameter
- **WHEN** a client lists tools from `esdiag mcp` or from `/mcp` in `user` mode
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

