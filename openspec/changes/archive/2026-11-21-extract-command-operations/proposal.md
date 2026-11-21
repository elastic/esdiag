## Why

Every finite command's behavior lives inside one `match` in the 6.3k-line `src/main.rs`, mixed with argument parsing, terminal prompts, and output rendering. Nothing outside the binary can call a command. The MCP server changes (`add-mcp-stdio`, `add-mcp-http`) need to run the same commands from `esdiag-core` and `esdiag-gui`, and copying the bodies would let the CLI and MCP drift apart.

## What Changes

- Add an `operations` module to `esdiag-core` with one function per finite command. Each takes typed arguments and returns a typed command outcome, and none parses arguments, prints, reads stdin, or prompts:
  - `collect`, `process` (including the `ask` follow-up), `send`
  - `host_add`, `host_update`, `host_remove`, `host_list`, `host_auth`
  - `job_list`, `job_run`, `job_delete`, `job_save` (derived from collect or process arguments)
  - `output_set`, `output_show`, `output_clear`
  - `setup_elasticsearch`, `setup_kibana`, `setup_output`, `setup_env`, `setup_context`
  - `agent_ask`
  - `keystore_status`, plus `keystore_add`, `keystore_update`, `keystore_remove`, `keystore_unlock`, `keystore_lock` taking already-resolved secret values
- Follow the `esdiag-core::onboarding` convention for credentials. Operations that change the keystore take a keystore password the caller already resolved. Implicit secret lookups keep going through `get_keystore_password`, which prompts only after the foreground CLI calls `enable_keystore_password_prompt()`. No new interaction-mode mechanism is added.
- Return a command outcome that distinguishes a typed `CliOutcome` from output streamed to stdout, so callers can detect and reject stdout export.
- Move failure classification (`classify_failure`, `structured_failure`, `safe_failure_message`, HTTP response details) and outcome builders (`collection_outcome`, `execution_process_result`, `job_outcome`, `host_result`, `saved_job_result`, `setup_outcome`) from `main.rs` to `esdiag-core`.
- Reduce `main.rs` to argument parsing, prompts that resolve operation arguments, tracing setup, dispatch, and outcome rendering.
- Remove the unused `job::handle_job_list`.
- Out of scope, remaining in the binary: `init` (interactive wizard), `local` (root package), `serve` and the desktop wrapper (`esdiag-gui`), `setup skills`, `setup completions`, `version`.
- No behavior change: help text, prompts, structured outcomes, failure categories, exit codes, and log messages and levels stay as they are. Only the log target shown on moved lines changes, from `esdiag` to `esdiag_core::operations::…`, as `extract-gui-crate` already did for library modules.

Stack position: after `extract-gui-crate` ([#467](https://github.com/elastic/esdiag/pull/467)), before `add-mcp-stdio` and `add-mcp-http`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. This is a pure refactor, so `.openspec.yaml` sets `skip_specs: true`. The `cli-structured-output` contract is unchanged and guards the move.

## Impact

- Target products: all (Elasticsearch, Kibana, Logstash, ECK), as an internal restructuring with no change to collection or processing output.
- Surfaces: CLI code moves into core library code. No Web UI change.
- Code: new `crates/esdiag-core/src/operations/`; `cli_output` gains failure classification and the command outcome type; `src/main.rs` shrinks; `job/mod.rs` loses `handle_job_list`.
- Tests: unit tests for moved logic move with it into `esdiag-core`. CLI integration tests in `tests/` stay unchanged as the behavior check.
- Build: edits to command logic rebuild `esdiag-core` instead of only the root package.
- Dependencies: none added.
- Docs: `docs/repository/organization.md` gains the operations module; `CHANGELOG.md` notes the log target change.
