## Context

See proposal.md for motivation. This design is written against `extract-gui-crate` as implemented in [#467](https://github.com/elastic/esdiag/pull/467) (branch `refactor/extract-gui-crate`). That PR archives its change to `openspec/changes/archive/2026-11-21-extract-gui-crate/` and syncs `openspec/specs/workspace-crate-architecture/`. Line numbers below refer to that branch.

- **Layout.** `crates/esdiag-core/` holds the library, including `cli_output`, `job`, `data`, `setup`, `agent`, and `onboarding`. `crates/esdiag-gui/` holds the server and Tauri wrapper. The root package holds `src/main.rs` (6,022 lines: about 4,080 of code, then a 1,940-line test module), `src/local.rs`, `src/local/`, and a facade `src/lib.rs` (`pub use esdiag_core::*;` plus `esdiag_gui::server`). The PR states that new library code belongs in the implementation crates, not the facade.
- **Layering.** `.github/workflows/workspace.yml` checks with `cargo metadata` that `esdiag-core` never depends on `esdiag-gui`. `default-members` is the root and `esdiag-core`, so plain `cargo test` skips `esdiag-gui`; `cargo test --workspace` runs everything.
- **Test support.** `TestEnv` and `write_hosts_yml_for_tests` are available as `#[cfg(any(test, feature = "test-support"))]` in `esdiag-core`. The Elasticsearch mock is `crates/esdiag-core/src/es_mock.rs`.
- **Command bodies.** `run(cli, format)` (lines 1428 to 2104) is the dispatch `match`. Command logic sits inline there and in helpers: `derive_collect_job`/`derive_process_job`, `build_host_from_definition`, `save_host`, `delete_host_from_cli`, `validate_host_connection`, `run_setup` and `run_stack_setup`, and `run_agent_ask_with_deployment`. Failure classification is `classify_failure` and `structured_failure` (lines 1040 to 1244). Outcome builders include `collection_outcome`, `execution_process_result`, `job_outcome`, `host_result`, `saved_job_result`, and `setup_outcome`.
- **`CommandResult`** is `struct CommandResult { outcome: Option<CliOutcome> }` in `main.rs`. `None` means the command streamed to stdout (process to `-`, a stdout-export saved job, server, local).
- **Keystore prompting** is already opt-in. `async_main` calls `esdiag::data::enable_keystore_password_prompt()` for foreground commands. After that, `get_keystore_password` may prompt on a terminal; otherwise it uses `ESDIAG_KEYSTORE_PASSWORD`, then the unlock lease, then fails. The GUI injects a password for a job with `with_scoped_keystore_password`. Keystore mutation commands resolve their password with `get_password_for_secret_commands`, and their secret values with `resolve_secret_input`, both in the binary.
- **Precedent.** `esdiag-core::onboarding` already provides flow-neutral operations: plain functions taking `*Input` structs and an explicit `keystore_password`. They never prompt, and both the terminal wizard (`run_init_wizard`) and the web onboarding call them.
- **Logging.** Tracing filters set levels only (`EnvFilter::new(level)`), and the default `fmt` layer prints the target. #467 recorded its target rename in `CHANGELOG.md`.
- **Lint baseline.** `cargo clippy --workspace --all-targets --all-features` reports five existing warnings on `main` and on #467.

## Goals / Non-Goals

**Goals:**

- Every finite command named in the proposal is callable from `esdiag-core` with typed inputs and a typed outcome. The binary is its first caller, and MCP and `esdiag-gui` can follow.
- Leave exactly one convention for library operations: the `onboarding` one.

**Non-Goals:**

- Making the web server call these operations. `esdiag-gui` keeps its own handlers; the MCP changes are the next callers.
- Moving `init`, `local`, `serve`, `setup skills`, `setup completions`, or `version`.
- Splitting operations into their own crate.
- Changing a trait, typestate, receiver, processor, or exporter. Operations build `Job`s and call `job::executor` exactly as `main.rs` does now, so no state transitions change.

## Decisions

### Module layout follows `onboarding`

```
crates/esdiag-core/src/operations/
  mod.rs        CommandOutcome, shared helpers (role checks, output display)
  collect.rs    collect, CollectInput
  process.rs    process, ProcessInput (incl. ask), derive_process_job
  send.rs       send, SendInput
  host.rs       host_add/update/remove/list/auth, HostInput, connection validation
  job.rs        job_list/run/delete/save            #[cfg(feature = "keystore")]
  output.rs     output_set/show/clear
  setup.rs      setup_elasticsearch/kibana/output/env/context   #[cfg(feature = "setup")]
  agent.rs      agent_ask                           #[cfg(feature = "agent")]
  keystore.rs   keystore_status/add/update/remove/unlock/lock
```

Each operation is a free function, `async` where its body awaits, taking one `*Input` struct of owned, already-parsed values and returning `Result<CommandOutcome>`. Input structs derive `Debug` with secrets in `Secret<String>`, so a later `schemars` derive for MCP adds no new types. Inputs use domain types (`HostRole`, `Application`, `Identifiers`), never clap types. Clap enums such as `AgentSkillTarget` stay in the binary, which converts them.

`onboarding` stays as it is. `run_init_wizard` calls `operations` where it currently duplicates a command body (host save, saved-job save, output setup), so each behavior has one implementation.

Alternatives considered:

- One method per command on an `Operations` struct holding configuration. Rejected: operations read configuration through `ApplicationConfig::load` and `KnownHost` as `main.rs` does now, and a struct would be a second convention next to `onboarding`.
- A separate `esdiag-operations` crate between core and its callers. It would keep command-logic edits from rebuilding core (see Risks). Rejected for now: #467 holds further splits until its timings settle, and the module can move later without changing callers' imports beyond one path.

### `CommandOutcome` replaces `CommandResult`

```rust
pub enum CommandOutcome {
    Completed(CliOutcome),
    Streamed,
}
```

It lives in `cli_output`. `main.rs` drops `CommandResult` and renders `Completed` with `write_terminal_outcome`. `local`, `serve`, `init`, and `version` keep returning through the same type from their binary-side functions. The variants name the two cases callers branch on, which `Option` hid. The MCP tools reject `Streamed` paths before executing, using `ProcessInput::writes_stdout()` and `job::saved_job_writes_stdout(name)`, both added here because `main.rs` already computes them inline (`stdout_owned`).

### Credentials: reuse the existing opt-in prompt and explicit passwords

No interaction-mode parameter. Two existing mechanisms cover every path:

| Path | Today | After |
|---|---|---|
| Secret lookup during collect/process/setup/agent (`credential.rs` → `get_keystore_password`) | Prompts only if the binary called `enable_keystore_password_prompt()` | Unchanged. Operations never call the enabler; the binary still does. |
| Keystore mutation (`keystore add/update/remove/unlock`) | `main.rs` calls `get_password_for_secret_commands` and `resolve_secret_input`, which may prompt | Binary resolves the keystore password and secret values, then passes them as `Secret<String>` fields, like `onboarding::save_output_deployment(input, keystore_password)` |
| `host add/update --secret` | `resolve_add_host_secret_auth` reads the keystore through `get_keystore_password` | Moves into `operations::host`, unchanged |
| Raw `--apikey/--password` on `host add/update` | Parsed in `main.rs` into `KnownHostCliUpdate` | `HostInput` carries them as `Option<Secret<String>>`; MCP will leave them `None` |

Operations call no `rpassword`, `stdin`, or `is_terminal`. A grep check in the task list enforces this for `operations/`.

Any process that never enables the prompt (`esdiag serve`, and later `esdiag mcp`) therefore gets the fail-closed path with no new code.

### Failures and outcome builders move to core

`classify_failure`, `safe_failure_message`, `parse_http_response_details`, `http_response_details`, `command_failure`, and `structured_failure` move to `cli_output` as public functions. `format_execution_failure`, `execution_process_result`, `collection_outcome`, `job_outcome`, `host_result`, `saved_job_result`, and `setup_outcome` move to `operations`. The error marker types they downcast (`ProcessAskStdoutConflict`, `JobExecutionFailure`, `SavedJobNotFound`, setup errors) move to the modules that raise them, so classification no longer depends on binary-local types.

### What stays in `main.rs`

Clap definitions and `parse_cli`, the extension profile, tracing setup, `command_owns_stdout`, `enable_keystore_password_prompt()`, all prompts (`prompt_*`, `resolve_secret_input*`, `get_password_for_secret_commands` call sites), `run_init_wizard`, `run_local_lifecycle`, `serve` resolution (`resolve_serve_*`), the desktop branch, `setup skills`/`completions`, `version`, and the conversions from clap arguments to `*Input`. After the move, each `run` arm is: build input, optionally prompt, call operation, return outcome.

### Tests move with their code

The 1,940-line `main.rs` test module has about 74 tests outside clap parsing and prompts. Tests of host building and redaction, setup outcomes and failures, agent failures, and failure classification move to the matching `operations` or `cli_output` module and use `crate::TestEnv`. The default-job tests exercise wizard helpers (`configured_default_job`, `keep_default_job`) and stay with `run_init_wizard`, as do the tests of clap parsing, the extension profile, tracing, completions, and prompts. Integration tests in `tests/` are not edited; they are the behavior check.

### Verification follows #467's method

Before and after the change, on the same commit base:

1. Help output for every command and subcommand is byte-identical.
2. Structured outcomes for host add/update/remove/list, job list/delete, output set/show/clear, and keystore status match against a `TestEnv` home, and `esdiag process` documents for every archive under `tests/archives/` match, masking run IDs and timestamps as #467 did.
3. Structured failures (category and message) for unknown host, unknown job, locked keystore without `ESDIAG_KEYSTORE_PASSWORD`, and stdout-ask conflict match.
4. `cargo test --workspace` passes, and `cargo clippy --workspace --all-targets --all-features` reports no warnings beyond the five on the base.

## Risks / Trade-offs

- [Command-logic edits now rebuild `esdiag-core`] → #467 measured a core edit at about 13.4 s versus 2.9 s for `main.rs`. About 1,500 to 1,800 lines of command logic move, so command edits pay the core cost; edits to clap, prompts, and init stay cheap. The `esdiag-gui` rebuild-scope requirement is unaffected. If this hurts, the alternative above (a separate crate) is the fix.
- [Log targets change for moved code] → Messages and levels are unchanged and filters are level-only. `CHANGELOG.md` notes it, following #467.
- [Classification relies on `downcast_ref` of marker types] → Moving the markers and the classifier together keeps every downcast resolvable. The structured-failure comparison in Verification catches a missed type, because a missed downcast falls back to `internal`.
- [Init wizard behavior shifts when it switches to operations] → Only replace calls whose bodies are identical today; anything that differs stays wizard-local. No Rust integration test drives `init`, so the wizard unit tests that stay in `main.rs` must pass unchanged, and the task list includes a manual `esdiag init` run against a `TestEnv`-style temporary home.
- [Rebasing on #467 review changes] → This change starts after #467 merges, and the line references above are re-checked then.

## Migration Plan

Internal and single-PR. Rollback is reverting the PR. No configuration, data, or user-facing migration.
