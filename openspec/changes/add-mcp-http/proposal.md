## Why

`esdiag mcp` serves one local client that launches it. A running `esdiag serve` instance, local or shared, cannot be reached by MCP clients, so an agent cannot submit work to the shared service-mode deployment that already processes uploads for a team.

## What Changes

- Mount a Streamable HTTP MCP endpoint at `/mcp` in the `esdiag-gui` web server, built when the `mcp` cargo feature and `server` are enabled.
- Add `mcp` as an optional web feature name for `serve --web-features` and `ESDIAG_WEB_FEATURES`. It is disabled by default in both runtime modes, and listing it in a build without MCP support fails startup.
- In `user` mode, `/mcp` serves the same tool catalog as `esdiag mcp`.
- In `service` mode, `/mcp` serves only `process`, restricted to Elastic Upload Service inputs and the startup-defined exporter. It is subject to the existing per-owner and global job concurrency caps. No tool reads or writes `hosts.yml`, `jobs.yml`, the keystore, or the output context.
- Authenticate `/mcp` requests with the configured request authentication provider and record the authenticated identity in the diagnostic's `Identifiers`, as for web jobs.
- Validate inbound `Host` and `Origin` headers against loopback by default, with an explicit allowlist (`--mcp-allowed-hosts`, `ESDIAG_MCP_ALLOWED_HOSTS`) for deployments behind a public hostname.
- Serve MCP statelessly, without server-held sessions, so service-mode replicas behind a load balancer need no session affinity.
- Not included: MCP OAuth authorization, MCP over the desktop app's ephemeral-port server, and service-mode tools beyond `process`.

Prerequisites, in stack order: `extract-gui-crate` ([#467](https://github.com/elastic/esdiag/pull/467)), `extract-command-operations`, and `add-mcp-stdio`, which provides the shared tool catalog in `esdiag-core`.

## Capabilities

### New Capabilities

- `mcp-http-server`: The `/mcp` endpoint: transport, enablement, mode-specific tool policy, authentication and identity, host and origin validation, and service-mode concurrency.

### Modified Capabilities

- `web-feature-policy`: The web feature allowlist gains the `mcp` feature name.
- `workspace-crate-architecture` (synced by #467): `mcp` with `server` compiles the HTTP transport only in `esdiag-gui`. The delta is added when this change is rebased onto #467.

## Impact

- Target products: Elasticsearch, Kibana, Logstash, and ECK diagnostics processed through the existing receivers and exporters. Processing output is unchanged.
- Surfaces: Web server (`esdiag serve`) gains an HTTP API endpoint, plus server policy and startup options. No Web UI page changes. CLI gains the `--mcp-allowed-hosts` option on `serve`.
- Code: `crates/esdiag-gui/src/server/` (router, policy, MCP service), `crates/esdiag-core/src/mcp/` (service-mode policy), root `src/main.rs` (`serve` option).
- Dependencies: `rmcp` with `transport-streamable-http-server` in `esdiag-gui`.
- Features: root `mcp` additionally forwards to `esdiag-gui?/mcp`.
- Docs: `serve` and web feature docs, the MCP usage page from `add-mcp-stdio`, and `CHANGELOG.md`.
