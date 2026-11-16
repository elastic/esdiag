## 1. Module Reorganization (no behavior change)

- [x] 1.1 Make `pub mod setup` ungated in `src/lib.rs`; split `src/setup.rs` into `src/setup/{mod,elasticsearch,kibana}.rs`, gating asset submodules on `setup` and keeping `esdiag::setup::{assets, assets_report, asset_statuses, assets_installed, SetupReport}`.
- [x] 1.2 Move `src/completions.rs` to `src/setup/completions.rs` (ungated) and `src/agent/skills.rs` to `src/setup/skills.rs` (gated on `agent`); update `use` paths in `src/main.rs`, `src/local.rs`, `src/server/*`, `src/client/kibana.rs`, and doc comments in `src/embeds.rs`.
- [x] 1.3 Verify `cargo check` for `--no-default-features`, `--no-default-features --features agent`, `--no-default-features --features setup`, default, and `--features desktop`; run `cargo clippy --all-targets` and `cargo test`.

## 2. Command Tree

- [x] 2.1 Replace `Commands::Setup { host }` with `Commands::Setup { command: SetupCommands }` (subcommand required) and `SetupCommands::{Elasticsearch, Kibana, Output, Env, Context, Skills, Completions}` with the per-variant feature gates from the design.
- [x] 2.2 Remove `Commands::Completions`, `CompletionsCommands`, and `AgentCommands::Skills`; implement `setup completions <SHELL> [--print]` and `setup skills [AGENT]... [--force]` over the existing code; update `command_owns_stdout` to match `--print`.
- [x] 2.3 Update the `esdiag init` skill-stage `recovery_command` and the "assets were not installed" hint to the new forms.
- [x] 2.4 Add `src/main.rs` parse tests for every new form, bare `setup` → help/non-zero, removed spellings rejected, and minimal-feature availability.

## 3. License, Shared Pipeline, And Saved Targets

- [x] 3.1 Split `ensure_agent_builder_license` into `src/setup/license.rs` `agent_builder_license_status` (read-only; `Managed` on serverless) and `activate_trial_license`; update `src/setup/serverless_tests.rs`, `tests/serverless_setup_tests.rs`, and `tests/elasticsearch_feedback_tests.rs`.
- [x] 3.2 Add the shared Elasticsearch → license → Kibana pipeline in `src/setup/mod.rs` taking `activate_trial: bool`; skip Kibana with a `partial` outcome and opt-in warning when the license is inactive and not opted in; emit skip warnings for undeclared services.
- [x] 3.3 Add `--activate-trial` to `output`, `env`, and `context` only, resolved flag → `ESDIAG_SETUP_ACTIVATE_TRIAL` → `--env-file` value (`TrialArgs::resolve`).
- [x] 3.4 Implement `setup output [NAME]` (explicit or persisted, `send` role, viewer optional) through `OutputDeployment::resolve_saved`, and `setup elasticsearch [HOST]` / `setup kibana [HOST]` with `app` validation; none read `ESDIAG_OUTPUT_*`. Expose persisted/explicit-only resolvers from `src/data/output_deployment.rs`.
- [x] 3.5 Update `tests/serverless_setup_tests.rs` (`setup kibana` → `setup kibana kibana`; bare `setup` → `setup env`) and add the license matrix (active / inactive × opted in / not × serverless, asserting no `start_trial` without opt-in) plus tests for `setup output` default, named, no viewer, env not redirecting, wrong-`app` rejection, and `--activate-trial` rejected on single-service commands.

## 3b. Onboarding

- [x] 3b.1 `esdiag init`: route asset setup through the shared pipeline; add the default-no trial prompt when an existing output's license is inactive; ask no trial question when starting a new local core stack (the local default applies); update the "assets were not installed" hint to `esdiag setup output <name>`.
- [x] 3b.2 Web onboarding: add the unchecked trial checkbox to `templates/welcome.html`, pass it through `install_output_assets` → `install_output_assets_stage`, and surface the licensing warning; add server tests for both choices.

## 4. Env Setup

- [x] 4.1 Add optional `dotenvy` behind the `setup` feature.
- [x] 4.2 Implement `src/setup/env.rs`: read the six `ESDIAG_*` keys from `--env-file` (non-mutating parse) overlaid by the process environment; validate auth combinations; build in-memory hosts for whichever URLs are declared.
- [x] 4.3 Implement `setup env [--env-file <PATH>]` over the shared pipeline.
- [x] 4.4 Add tests: env only, file only, env overrides file, ignored unrelated keys, no implicit `./.env`, Elasticsearch-only, Kibana-only, none declared, conflicting auth, and no secret in the outcome.

## 5. Context Setup

- [x] 5.1 Add optional `elasticrc` path dependency behind the `setup` feature.
- [x] 5.2 Implement `src/setup/context.rs`: load config (`--config`, `ELASTIC_CLI_CONFIG_FILE`, home), select named or current context, resolve whichever of `elasticsearch`/`kibana` are defined, and select the Kibana space with `ESDIAG_KIBANA_SPACE`.
- [x] 5.3 Implement `setup context [NAME] [--config <PATH>]` over the shared pipeline.
- [x] 5.4 Add fixture `.elasticrc.yml` + mock-server tests: full, Elasticsearch-only, Kibana-only, cloud-only failure, unknown context, `env` resolver, and `hosts.yml`/keystore/outcome contain no secrets.

## 6. Local Stack And Containers

- [x] 6.1 Add `setup_command_for_image` to `src/local.rs` and use it in the compose template; add unit tests for `0.17.0`, `0.17.9`, `0.18.0`, `0.18.0-SNAPSHOT`, `0.19.1`, and a custom `dev` tag.
- [x] 6.2 Add `setup_command_args` to `bin/esdiag-local`, use it in `write_compose`, and change core mode `run_setup` to `"$binary" setup env`.
- [x] 6.3 Add `--activate-trial=true|false` with `ESDIAG_SETUP_ACTIVATE_TRIAL` fallback to `LocalOptions` in `src/local.rs` and to `bin/esdiag-local` option parsing and usage. When both are absent, resolve auto before setup with `GET _license/trial_status` (`reqwest` in `src/local.rs`, so full mode needs no `setup` feature; `curl` with jq/sed fallback in Bash): `true` only when `eligible_to_start_trial` is `true`, `false` on failure. Pass the resolved value as `-e ESDIAG_SETUP_ACTIVATE_TRIAL=<v>` on `compose run` (full), in the native `setup env` environment (`bin/esdiag-local` core), and into the in-process pipeline (`src/local.rs` core). Never persist it.
- [x] 6.4 Update `tests/esdiag-local.sh`: the fake `esdiag` logs its full arguments and `ESDIAG_SETUP_ACTIVATE_TRIAL`. Core mode asserts `setup env`. Compose assertions cover the current image and a 0.17.x-pinned state using the same version table as 6.1. A fake `curl` serves `trial_status`. Trial cases cover auto when eligible (`true`), auto when not eligible (`false`), auto when the request fails (`false`), `--activate-trial=false` on a new stack, the env var overriding eligibility, and `--activate-trial=true`. Each asserts the value is absent from `.env` and `compose.yml`. Mirror the same table in `src/local.rs` unit tests. Add an `--activate-trial=true` forwarding case to `tests/bin/esdiag-control.sh`.
- [x] 6.5 Run `bash tests/esdiag-local.sh`, `bash tests/bin/esdiag-control.sh`, `shellcheck bin/esdiag-local`, and `bash -n` on the rendered release script.
- [x] 6.6 Build the container image locally; verify `docker run … <image> setup env` against a local stack, and `esdiag local up --stack=full` / `--stack=core` end to end. Confirm a new stack activates the trial and installs Agent Builder assets, a second `up` sends no `start_trial`, `--activate-trial=false` on a fresh stack skips it, and a state directory created by 0.17.x works before and after `esdiag local upgrade`.

## 7. Skill, Plugin, And Docs

- [x] 7.1 Update `.agents/skills/esdiag/references/esdiag-cli.md`, run `bin/sync-plugin-skill.sh`, then `bin/sync-plugin-skill.sh --check`.
- [x] 7.2 Update `tests/plugin.sh` to assert `esdiag setup skills`; run `bash tests/plugin.sh`.
- [x] 7.3 Update `plugin/README.md`, `docs/command-line.md`, `docs/setup/{installation,agent-skills,remote-cluster,serverless,shared-service,index,local-stack}.md`, `docs/bin/esdiag-local.md`, `docs/reference/serverless-assets.md`, `docs/repository/organization.md`, `assets/readme.md`, and `example.env` per the integration inventory.
- [x] 7.4 Replace the `[Unreleased]` CHANGELOG entry from #430 and add Breaking entries mapping every old spelling to its replacement, including the `setup kibana`/`setup elasticsearch` saved-host retarget, and the change to an opt-in trial license (`--activate-trial` / `ESDIAG_SETUP_ACTIVATE_TRIAL`) for manual setup, `esdiag init` on existing outputs, and web onboarding, while new local stacks still activate one by default.

## 8. Verification And Archive

- [x] 8.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features`, and record results in the PR.
- [x] 8.2 Run `openspec validate consolidate-setup-subcommands --strict`.
- [x] 8.3 Run `openspec archive consolidate-setup-subcommands` before merge so `openspec-archived.yml` passes.
