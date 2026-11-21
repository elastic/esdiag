## 1. Prerequisites

- [ ] 1.1 Rebase onto merged #467 and `extract-command-operations`; verify each catalog command has an `esdiag-core::operations` function returning `Result<CommandOutcome>`, and that `ProcessInput::writes_stdout` and `saved_job_writes_stdout` exist
- [ ] 1.2 Add a `workspace-crate-architecture` delta modifying `Cargo Features Are Preserved`: add `mcp` to the feature list and to the "Each feature builds alone" scenario, and add a scenario that `mcp` alone compiles neither `esdiag-gui` nor the rmcp HTTP transport; verify `openspec validate add-mcp-stdio --strict` passes

## 2. Feature and dependencies

- [ ] 2.1 Add `mcp` to `esdiag-core` with `rmcp` (`server`, `macros`) as an optional dependency, and the root `mcp` feature forwarding to it with `rmcp` (`transport-io`); add `mcp` to `full`; verify `cargo tree -e normal --no-default-features` contains no `rmcp`
- [ ] 2.2 Verify `cargo build --no-default-features --features mcp` and `cargo build` both succeed

## 3. Tool catalog

- [ ] 3.1 Create `esdiag-core::mcp` with `EsdiagMcp`, `McpPolicy::Local`, server info (`esdiag`, crate version), and outcome and failure conversion to `CallToolResult`; unit-test that a `CliFailure` maps to an error result with `category` preserved
- [ ] 3.2 Add read-only tools `host_list`, `host_auth`, `job_list`, `output_show`, `keystore_status` with annotations; test against `TestEnv` that `host_list` structured content equals the `--format json` outcome
- [ ] 3.3 Add `process`, `collect`, `job_run`, `agent_ask`, including the `save_job` parameter, the `output: "-"` rejection, and the stdout-export saved-job rejection; test that `process` on a fixture archive to a directory output returns `diagnostic_processed`, and that `-` and a stdout-export saved job each return `invalid_input`
- [ ] 3.4 Add `send`, `setup_output`, `setup_elasticsearch`, `setup_kibana`, `host_add`, `host_update`, `output_set`; test that the `host_add` and `host_update` schemas contain `secret` and no `apikey`, `username`, or `password`
- [ ] 3.5 Add destructive tools `host_remove`, `job_delete`, `output_clear` with destructive annotations; test that `tools/list` annotations match the catalog table
- [ ] 3.6 Gate tools on `keystore`, `agent`, and `setup`; verify `cargo test --no-default-features --features mcp` passes a tool-list test that expects only ungated tools
- [ ] 3.7 Add the configuration-write mutex; test that concurrent `host_add` calls for two names both persist
- [ ] 3.8 Forward `ExecutionEvent`s as progress notifications and race operations against the request cancellation token; test with an in-process rmcp client that `process` with a progress token receives a notification before its result, and that a cancelled call returns no success while a later `host_list` succeeds
- [ ] 3.9 Test that a keystore-backed host with a locked keystore and no `ESDIAG_KEYSTORE_PASSWORD` fails with `keystore_failed` without reading stdin

## 4. Stdio subcommand

- [ ] 4.1 Add `Commands::Mcp` under `#[cfg(feature = "mcp")]`, route tracing to stderr, add it to `command_owns_stdout`, exclude it from `enable_keystore_password_prompt()`, and serve `EsdiagMcp` over `stdio_pipes()` until end of input or a signal; verify `esdiag --help` lists `mcp`
- [ ] 4.2 Add an integration test in `tests/` that spawns `esdiag --debug mcp`, performs `initialize`, `tools/list`, and a `process` call on a fixture, asserts every stdout line parses as JSON-RPC, and asserts exit status zero after closing stdin

## 5. Plugin, docs, and changelog

- [ ] 5.1 Add `plugin/.mcp.json` and reference it from the Codex manifest after confirming the Codex plugin schema field; verify both manifests parse and the registration contains only `esdiag` and `mcp`
- [ ] 5.2 Update `.agents/skills/esdiag/` to prefer MCP tools when present and fall back to the CLI, then run `bin/sync-plugin-skill.sh`; verify the drift check passes
- [ ] 5.3 Add an MCP usage page under `docs/` covering client registration, the keystore unlock requirement, and the tool list, and link it from the docs index; run `okf validate docs/`
- [ ] 5.4 Add the `CHANGELOG.md` entry following `.agents/skills/changelog/SKILL.md`

## 6. Verification

- [ ] 6.1 Run `cargo test --workspace` and `cargo clippy --workspace --all-targets --all-features`; tests pass and clippy reports no warnings beyond the base branch's count
- [ ] 6.2 Verify the `workspace.yml` layering checks pass locally: `esdiag-core` with `mcp` does not depend on `esdiag-gui`
- [ ] 6.3 Register `esdiag mcp` in Claude Code, run `host_list` and `job_run` on a saved job, and confirm the structured results and progress display
