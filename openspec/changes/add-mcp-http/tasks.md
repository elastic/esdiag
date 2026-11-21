## 1. Prerequisites

- [ ] 1.1 Rebase onto merged `add-mcp-stdio`; verify `esdiag-core::mcp::EsdiagMcp` and `McpPolicy::Local` exist and `cargo test --workspace` passes
- [ ] 1.2 Add a `workspace-crate-architecture` delta stating that `mcp` without `server` compiles no `esdiag-gui`, and that `mcp` with `server` compiles the HTTP transport only in `esdiag-gui`; verify `openspec validate add-mcp-http --strict` passes

## 2. Feature and policy

- [ ] 2.1 Add the `mcp` feature to `esdiag-gui` (`esdiag-core/mcp`, `rmcp` with `transport-streamable-http-server`) and forward root `mcp` to `esdiag-gui?/mcp`; verify `cargo build --no-default-features --features server` compiles no `rmcp` HTTP transport and `cargo build` succeeds
- [ ] 2.2 Add `WebFeature::Mcp`, its disabled-by-default rule in both modes, the build-support startup error, and `ServerPolicy::allows_mcp()`; extend the web feature policy tests for every modified `web-feature-policy` scenario
- [ ] 2.3 Add `--mcp-allowed-hosts` and `ESDIAG_MCP_ALLOWED_HOSTS` resolution with the loopback default and the non-loopback bind warning; unit-test precedence and origin derivation

## 3. Core service policy

- [ ] 3.1 Define the `McpHost` trait, `AdmissionGuard`, and `McpPolicy::Service` in `esdiag-core::mcp`, moving `JobAdmissionError` and `RequestIdentity` into core if not already there; verify `cargo tree -p esdiag-core` still excludes `esdiag-gui`
- [ ] 3.2 Implement the service-mode `process` tool with `input`-only parameters, Elastic Upload Service input validation, and the host exporter; test with a fake `McpHost` that the tool list is only `process`, that a local path returns `invalid_input` without filesystem access, and that a cap error returns a tool error naming the cap
- [ ] 3.3 Route identity from `McpHost` into job `Identifiers` for both policies; test that a fake identity appears in the processed report's user

## 4. GUI endpoint

- [ ] 4.1 Implement `McpHost` on `ServerState` (identity extension, `record_job_started` admission with release on drop, startup exporter), and make `require_authenticated_user` insert `RequestIdentity`; test that admission releases the slot when a call fails
- [ ] 4.2 Mount `/mcp` with `StreamableHttpService` (stateless, JSON responses, shutdown token, allowed hosts and origins) inside the authentication layer, plus the `/mcp` host and origin guard, selecting `Local` or `Service` policy from runtime mode; test with `portpicker` and `reqwest` that `/mcp` is not found without the feature and that `initialize` succeeds with it
- [ ] 4.3 Add integration tests for: `Host: attacker.example` returns 403; a mismatched `Origin` returns 403; service mode with the identity-aware proxy provider rejects a request without the identity header; user-mode `tools/list` matches `esdiag mcp`; `process` with a progress token returns an event stream with progress before the result

## 5. Docs and changelog

- [ ] 5.1 Update `serve` docs and the web feature docs for `mcp` and `--mcp-allowed-hosts`, and add the HTTP section to the MCP usage page with service-mode limits; run `okf validate docs/`
- [ ] 5.2 Add the `CHANGELOG.md` entry following `.agents/skills/changelog/SKILL.md`

## 6. Verification

- [ ] 6.1 Run `cargo test --workspace` (which includes `esdiag-gui`) and `cargo clippy --workspace --all-targets --all-features`; tests pass and clippy reports no warnings beyond the base branch's count
- [ ] 6.2 Run `esdiag serve --web-features advanced,mcp`, register `http://127.0.0.1:2501/mcp` in Claude Code and Codex, and confirm `host_list` and a `process` call with progress work without a session ID
- [ ] 6.3 Run `esdiag serve --mode service --web-features mcp` with an output exporter, call `process` with an Elastic Upload Service link, and confirm the diagnostic lands in the startup exporter with the request identity
