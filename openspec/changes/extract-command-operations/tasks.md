## 1. Baseline

- [x] 1.1 Rebase onto merged #467; re-check the `main.rs` line references in design.md and verify `crates/esdiag-core/src/onboarding.rs` and `TestEnv` behind `test-support` exist
- [x] 1.2 Build the pre-refactor binary and record its help output for every command, the structured outcomes and failures listed in design.md (Verification, items 2 and 3), `esdiag process` documents for every archive under `tests/archives/`, and the clippy warning count; verify the recorded files exist outside the repository

## 2. Shared types

- [x] 2.1 Add `CommandOutcome` to `cli_output`, replace `CommandResult` in `main.rs` with it, and verify `cargo build` and `cargo test` pass with no behavior change
- [x] 2.2 Move failure classification (`classify_failure`, `safe_failure_message`, HTTP response details, `command_failure`, `structured_failure`) and the error marker types it downcasts into `esdiag-core`; move their unit tests and verify `cargo test -p esdiag-core` runs them
- [x] 2.3 Create `operations/mod.rs` with shared helpers (role checks, safe output display) and the outcome builders; verify the moved builder tests pass in `esdiag-core`

## 3. Operations

- [x] 3.1 Move `send` and `output_set/show/clear`; reduce their `run` arms to input construction and one call; verify `cargo test --workspace`
- [x] 3.2 Move `host_add/update/remove/list/auth`, including host building, secret-reference resolution, connection validation, and settings cleanup after delete; move their tests; verify `cargo test --workspace`
- [x] 3.3 Move `job_list/run/delete/save` and `derive_collect_job`/`derive_process_job`, add `saved_job_writes_stdout`, and delete `job::handle_job_list`; verify `cargo test --workspace` and `cargo build --no-default-features`
- [x] 3.4 Move `collect` and `process` (including `ask` and `ProcessInput::writes_stdout`); verify `cargo test --workspace` and that the archive processing comparison from 1.2 matches
- [x] 3.5 Move `setup_elasticsearch/kibana/output/env/context` and `run_stack_setup`; move setup tests; verify `cargo build --no-default-features --features setup` and `cargo test --workspace`
- [x] 3.6 Move `agent_ask` with its failure handling and Kibana helpers; move agent tests; verify `cargo build --no-default-features --features agent`
- [x] 3.7 Move `keystore_status/add/update/remove/unlock/lock` with keystore password and secret values passed in as resolved `Secret<String>` fields; keep all prompts in `main.rs`; verify `cargo build --no-default-features --features keystore` and `cargo test --workspace`
- [x] 3.8 Switch `run_init_wizard` to the operations whose bodies it duplicates; verify the wizard unit tests in `main.rs` pass unchanged and that a manual `esdiag init` with `HOME` set to a temporary directory completes and writes the same `hosts.yml`, `jobs.yml`, and settings as the baseline binary

## 4. Guards and cleanup

- [ ] 4.1 Verify `rg -n "rpassword|stdin\(\)|is_terminal|enable_keystore_password_prompt|println!|print!\(" crates/esdiag-core/src/operations` returns nothing
- [ ] 4.2 Verify `main.rs` contains no remaining command bodies for the moved commands (each `run` arm builds an input, optionally prompts, and calls one operation) and no unused helpers (`cargo build` shows no dead-code warnings)
- [ ] 4.3 Verify each feature builds alone (`agent`, `server`, `setup`, `keystore`, `elasticrc`, and none) and that the `workspace.yml` layering checks pass locally with `cargo metadata`

## 5. Docs and changelog

- [ ] 5.1 Add `crates/esdiag-core/src/operations/` to `docs/repository/organization.md` and run `okf validate docs/`
- [ ] 5.2 Add a `CHANGELOG.md` line for the log target change following `.agents/skills/changelog/SKILL.md`

## 6. Verification

- [ ] 6.1 Compare against the 1.2 baseline: help output byte-identical; structured outcomes, failures, and processed documents equal after masking run IDs and timestamps
- [ ] 6.2 Run `cargo test --workspace` and `cargo clippy --workspace --all-targets --all-features`; tests pass and clippy reports no warnings beyond the baseline count
