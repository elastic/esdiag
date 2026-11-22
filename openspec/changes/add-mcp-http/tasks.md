## 1. Prerequisites

- [ ] 1.1 Rebase onto merged `add-mcp-stdio`; verify `esdiag-core::mcp::EsdiagMcp` and `McpPolicy::Local` exist and `cargo test --workspace` passes
- [x] 1.2 Add a `workspace-crate-architecture` delta stating that `mcp` without `server` compiles no `esdiag-gui`, and that `mcp` with `server` compiles the HTTP transport only in `esdiag-gui`; verify `openspec validate add-mcp-http --strict` passes

## 2. Feature and policy

- [ ] 2.1 Add the `mcp` feature to `esdiag-gui` (`esdiag-core/mcp`, `rmcp` with `transport-streamable-http-server`) and forward root `mcp` to `esdiag-gui?/mcp`; verify `cargo build --no-default-features --features server` compiles no `rmcp` HTTP transport, `cargo tree -p esdiag-core --features mcp` shows `rmcp` without it, and `cargo build` succeeds
- [ ] 2.2 Add `WebFeature::Mcp`, its disabled-by-default rule in both modes, the build-support startup error, and `ServerPolicy::allows_mcp()`; extend the web feature policy tests for every modified `web-feature-policy` scenario
- [ ] 2.3 Add `--mcp-allowed-hosts` and `ESDIAG_MCP_ALLOWED_HOSTS` resolution with the loopback default and the non-loopback bind warning; unit-test precedence and origin derivation
- [ ] 2.4 Add `ServerStartOptions::mcp_endpoint`, false by default and set only by `esdiag serve`; when false, never mount `/mcp` and warn if `mcp` is listed; test that `Server::start` with `ESDIAG_WEB_FEATURES=advanced,mcp` returns not found for `/mcp`

## 3. Core service policy

- [ ] 3.1 Define the `McpHost` trait (`identity` from request headers, `admit`, `exporter`), `AdmissionGuard`, and `McpPolicy::Service` in `esdiag-core::mcp`, moving `JobAdmissionError` and `RequestIdentity` into core if not already there; verify `cargo tree -p esdiag-core` still excludes `esdiag-gui`
- [ ] 3.2 Add `UploadLink::parse(url, token)`, which builds `Uri::ServiceLink` without `Uri::try_from`, saved hosts, the keystore, or the filesystem; unit-test acceptance of an `https://upload.elastic.co/...` URL and rejection of paths, directories, a saved host name with a `hosts.yml` present, non-upload hosts, `http`, query or fragment, embedded userinfo, and an empty token, checking that no failure message contains the token
- [ ] 3.3 Make `UploadServiceDownloader` async with the non-blocking `reqwest` client and move the web `service_link` route onto it; test against a local stalled server that dropping the download future closes the connection
- [ ] 3.4 Add `operations::process::process_prepared(input, exporter, identifiers, context) -> ExecutionOutcome`, which takes any already-built `Input` and resolves no local defaults, and make `esdiag-gui`'s `execute_synchronous_processing` a thin wrapper over it; existing `service_link` and API-key collection tests pass unchanged, including parent and included-diagnostic results
- [ ] 3.5 Add an execution-scoped `TaskTracker` and `CancellationToken` to `ExecutionContext`; spawn `ElasticsearchExporter::batch_tx` tasks through the tracker check the token before each `batch_send` sub-batch and before each `send_value_batch` HTTP 429 retry, and make the retry backoff end early on cancellation; test against a mock bulk endpoint that a cancelled export sends no sub-batch after cancellation, that cancelling during the backoff after a 429 sends no retry and returns before the backoff elapses, and that `TaskTracker::wait()` returns once the in-flight request ends
- [ ] 3.6 Make `AdmissionGuard` release its slot only after its execution's tracker is empty: on drop it cancels the token, closes the tracker, and spawns the release after `TaskTracker::wait()`; test with a fake `McpHost` that the slot stays taken while a tracked task runs and is released after it ends, for both a completed and a dropped call
- [ ] 3.7 Implement the service-mode `process` tool with `url` and `token` parameters, `UploadLink::parse`, admission held by the operation future, and `process_prepared` with the host exporter; test with a fake `McpHost` that the tool list is only `process`, that a local path and a saved host name return `invalid_input` without local-state access, and that a cap error returns a tool error naming the cap
- [ ] 3.8 Route identity from `McpHost` into job `Identifiers` and admission ownership for both policies; test that a fake identity appears in the processed report's user and account

## 4. GUI endpoint

- [ ] 4.1 Implement `McpHost` on `ServerState` (`resolve_identity` on the request headers for every provider, `record_job_started` admission with release on drop, startup exporter); test that admission releases the slot when a call fails, and that provider `none` with `ESDIAG_USER` and `ESDIAG_ACCOUNT` set records that user and account and counts the job against that owner
- [ ] 4.2 Mount `/mcp` with `StreamableHttpService` (stateless, JSON responses, shutdown token, allowed hosts and origins) inside the authentication layer, plus the `/mcp` host and origin guard, selecting `Local` or `Service` policy from runtime mode; test with `portpicker` and `reqwest` that `/mcp` is not found without the feature and that `initialize` succeeds with it
- [ ] 4.3 Add integration tests for: `Host: attacker.example` returns 403; a mismatched `Origin` returns 403; service mode with the identity-aware proxy provider rejects a request without the identity header; user-mode `tools/list` matches `esdiag mcp`; `process` with a progress token returns an event stream with progress before the result
- [ ] 4.4 Add service-mode integration tests with a per-owner cap of 1, for a stalled mock download and for a bulk export whose second sub-batch stalls: after a client disconnect, and after server shutdown, the call reports no success, no request reaches the mocks after cancellation besides the one in flight, and a new call from the same owner is rejected until that request ends and admitted afterwards

## 5. Docs and changelog

- [ ] 5.1 Update `serve` docs and the web feature docs for `mcp` and `--mcp-allowed-hosts`, and add the HTTP section to the MCP usage page with service-mode limits, the `url` and `token` inputs, and the desktop exclusion; run `okf validate docs/`
- [ ] 5.2 Add the `CHANGELOG.md` entry following `.agents/skills/changelog/SKILL.md`

## 6. Verification

- [ ] 6.1 Run `cargo test --workspace` (which includes `esdiag-gui`) and `cargo clippy --workspace --all-targets --all-features`; tests pass and clippy reports no warnings beyond the base branch's count
- [ ] 6.2 Run `esdiag serve --web-features advanced,mcp`, register `http://127.0.0.1:2501/mcp` in Claude Code and Codex, and confirm `host_list` and a `process` call with progress work without a session ID
- [ ] 6.3 Run `esdiag serve --mode service --web-features mcp` with an output exporter, call `process` with an Elastic Upload Service URL and its token, and confirm the diagnostic lands in the startup exporter with the request identity
