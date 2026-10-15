## Context

`esdiag setup` grew out of "install Elasticsearch templates" and now covers Kibana, Agent Builder licensing, and serverless differences. Today it chooses one of three behaviors implicitly:
- With a saved `HOST`, it installs that host's product (Elasticsearch or Kibana, by `app`).
- With `ESDIAG_OUTPUT_URL` set, it installs both products from environment variables and starts a trial license.
- Otherwise, it does the same against the persisted default output and its viewer.

Separately, 0.17.0 shipped `esdiag agent skills`, and #430 (unreleased) adds `esdiag completions`. Issue #431 asks for one verb, `setup`, whose subcommand names the external system or configuration source.

The command has more external callers than its CLI definition suggests. Containers, two launcher implementations, shell tests, onboarding prompts, the embedded skill, and offline docs all spell it out. The integration inventory below is the authoritative list of what this change must touch.

## Goals / Non-Goals

**Goals:**

- Give every external-configuration action one discoverable, explicit home, `esdiag setup <target>`, with matching `src/setup/<target>.rs` modules.
- Make the configuration source explicit: a saved host, the persisted output, environment variables / `.env`, or an Elastic CLI context.
- Keep `esdiag local` and `esdiag-local` working across upgrades of existing state directories.

**Non-Goals:**

- Compatibility aliases for removed spellings. ESDiag is pre-1.0, and this is a clean break.
- Changing asset content, installation order, rollover rules, or serverless handling.
- Changing the `esdiag local` / `esdiag-local` command surface, or their parity.
- Writing `.elasticrc` or `.env` files, or importing either into `hosts.yml` or the keystore.
- Moving the canonical skill out of `.agents/skills/esdiag/`. Repository agents, `bin/sync-plugin-skill.sh`, `.claude-plugin/`, and `EsdiagSkillAssets` depend on that path.

## Decisions

### 1. Command grammar

```text
esdiag setup elasticsearch [HOST]
esdiag setup kibana [HOST]
esdiag setup output [NAME] [--activate-trial]
esdiag setup env [--env-file <PATH>] [--activate-trial]
esdiag setup context [NAME] [--config <PATH>] [--activate-trial]
esdiag setup skills [AGENT]... [--force]       # AGENT: claude | codex | opencode
esdiag setup completions <SHELL> [--print]     # SHELL: bash | zsh | fish | powershell
```

`Commands::Setup { command: SetupCommands }` uses a required subcommand (`subcommand_required = true`, `arg_required_else_help = true`). There is no positional host on `setup` itself. Removed spellings (`setup <HOST>`, bare `setup`, `agent skills`, top-level `completions`) fail as Clap usage errors, with no special handling.

A saved host whose name matches a subcommand (for example `kibana`) is targeted explicitly as `esdiag setup kibana kibana`. Scripts that ran `esdiag setup kibana` against such a host now target the persisted output's viewer instead. The CHANGELOG calls this out as a breaking change.

### 2. Saved outputs: `setup output`, and defaults for `elasticsearch` and `kibana`

`setup output [NAME]` resolves a saved output deployment through `OutputDeployment::from_explicit(NAME)` when `NAME` is given, or `from_persisted` when it is omitted. That deployment is an Elasticsearch host with role `send`, plus its linked Kibana viewer. It runs the combined pipeline (Decision 5). If the output has no linked viewer, it sets up Elasticsearch only and notes the missing viewer in `warnings` without making the outcome `partial`, matching `env` and `context`. `setup output` replaces bare `esdiag setup` with no environment, and it is the command to re-run after an upgrade.

When `HOST` is omitted, `setup elasticsearch` and `setup kibana` resolve the same persisted default output: its Elasticsearch, or its linked viewer. None of the saved-output commands read `ESDIAG_OUTPUT_*`. Environment-driven setup is requested explicitly with `setup env`, so an exported variable can no longer redirect a command aimed at a saved output.

`OutputDeployment::resolve` keeps its current precedence for `process`, `serve`, and `agent ask`. Only `setup` stops consulting the environment implicitly.

### 3. `setup env` and `.env` files

Inputs:
- `ESDIAG_OUTPUT_URL`
- `ESDIAG_OUTPUT_APIKEY`, or `ESDIAG_OUTPUT_USERNAME` + `ESDIAG_OUTPUT_PASSWORD`
- `ESDIAG_KIBANA_URL`
- `ESDIAG_KIBANA_SPACE`

These are the same variables and the same auth-combination validation as `OutputDeployment::from_environment`.

- `--env-file <PATH>` reads `KEY=VALUE` lines, with `#` comments, optional `export ` prefixes, and single- or double-quoted values. It uses `dotenvy::from_path_iter`, which parses without mutating the process environment. Only the keys above are read, and the others are ignored. Values are never exported to child processes and never interpolated through command execution.
- Process environment variables override `--env-file` values, matching Docker Compose's `.env` precedence. Without `--env-file`, no file is read: there is no implicit `./.env` discovery.
- **Partial deployments are supported.** `ESDIAG_OUTPUT_URL` declares Elasticsearch, and `ESDIAG_KIBANA_URL` declares Kibana. Kibana-only setup authenticates with the `ESDIAG_OUTPUT_*` credentials. If neither URL is declared, the command fails before any request, naming both variables.
- `esdiag local`'s generated state `.env` is a valid `--env-file` input for host-side setup, once its in-container URLs are overridden.

### 4. `setup context`

- Load with `elasticrc::ConfigFile::load_with_options(config, None)`. This honors `--config`, then `ELASTIC_CLI_CONFIG_FILE`, then home discovery.
- Select `config.context(name)` or `config.current()`.
- **Partial contexts are supported.** Set up each of `elasticsearch` and `kibana` that the context defines, and ignore `cloud`. If the context defines neither, fail before mutation, naming the context.
- Resolve each service with `ServiceConfig::resolve()`. Map `elasticrc::Auth::{ApiKey, Basic}` into in-memory `KnownHost`s (`Application::Elasticsearch` + `HostRole::Send`, `Application::Kibana` + `HostRole::View`). Nothing is persisted. Resolver failures (`cmd`, `pass`, keychain) report the resolver kind, never a secret.
- The Kibana space comes from `ESDIAG_KIBANA_SPACE` (default `esdiag`), as for every other command, so installed assets and generated links share one space. A `/s/<space>` path in the context's Kibana URL does not select the space.

### 5. Stage order and opt-in trial activation

`output`, `env`, and `context` share one pipeline in `src/setup/pipeline.rs`: Elasticsearch assets, then the license stage, then Kibana assets. Starting an Elasticsearch trial is a one-time, irreversible license change on the target cluster. It must never happen as a side effect of re-running setup during an upgrade.

`ensure_agent_builder_license` is split:
- `agent_builder_license_status(&Client) -> LicenseStatus { Active, Inactive, Managed }`. This is read-only: `GET _license`, or `Managed` on serverless without a request.
- `activate_trial_license(&Client)`. This calls `POST _license/start_trial?acknowledge=true` and verifies the license afterward.

The license stage runs only when both services are present:

| License status | Opted in | Effect |
|---|---|---|
| `Active` / `Managed` | either | continue to Kibana |
| `Inactive` | yes | activate trial, verify, continue to Kibana |
| `Inactive` | no | skip the Kibana stage; outcome `partial`, with a warning naming `--activate-trial` and `ESDIAG_SETUP_ACTIVATE_TRIAL=true` |

Skipping the Kibana stage, instead of attempting it, is deliberate. The Kibana bundle syncs as one unit and contains Enterprise-only Agent Builder tools, skills, and workflows, so attempting it on a basic license fails partway with a Kibana error that doesn't say "license". The user re-runs once the license is in place, or opts in.

Opt-in sources, resolved by `TrialArgs::resolve` in `src/main.rs`, highest first:
- the `--activate-trial` flag;
- `ESDIAG_SETUP_ACTIVATE_TRIAL` in the process environment;
- for `setup env` only, the same key in `--env-file`.

The flag is a plain Clap `bool` rather than `#[arg(env = …)]`, because Clap cannot layer an env-file value under the process environment. The variable accepts `true`/`false`, `1`/`0`, `yes`/`no`, and `on`/`off`, and any other value is a usage error.

The flag exists only on `output`, `env`, and `context`. `setup elasticsearch` and `setup kibana` never read the license and never start a trial.

| Invocation | Elasticsearch assets | License stage | Kibana assets |
|---|---|---|---|
| `output` / `env` / `context`, both services | yes | yes | yes, if the license allows it |
| `output` / `env` / `context`, Elasticsearch only | yes | no | no |
| `env` / `context`, Kibana only | no | no | yes |
| `setup elasticsearch [HOST]` | yes | no | no |
| `setup kibana [HOST]` | no | no | yes |

The `setup_completed` outcome lists only the targets that were set up. When a partial source omits a service, the outcome adds a `warnings` entry naming the skipped service. In that case `outcome` reflects asset results only: a deliberately partial source is not `partial`. A Kibana stage skipped for licensing *is* `partial`, because the source declared Kibana.

Interactive callers apply the same rule:
- **`esdiag init`** (`src/main.rs` ~L2548): after the existing "install dashboards and agents?" prompt, when the license status is `Inactive`, ask "Start a 30-day Elasticsearch trial license on `<output>` for Agent Builder? [y/N]". Declining installs Elasticsearch assets and skips Kibana with the same warning. Core configuration stays valid either way.
- **Web onboarding** (`install_output_assets_stage`, `templates/welcome.html`): add an unchecked "Start an Elasticsearch trial license" checkbox to the asset-install step, and pass its value into the pipeline. When the license is inactive and the box is unchecked, the step reports the licensing warning instead of a Kibana error.

### 6. Local stack: version-aware compose command

Both launchers regenerate `compose.yml` on every `up`, `setup`, and `upgrade`:
- `src/local.rs`: `initialize` calls `write_compose`, and `upgrade` calls it too.
- `bin/esdiag-local`: `init_state` calls `write_compose`.

Both keep the `ESDIAG_IMAGE` already pinned in the state `.env` until `upgrade`. A 0.18 launcher running against a 0.17-pinned state therefore writes a compose file for a 0.17 image. If that file said `setup env`, the old binary would treat `env` as a host name and fail during `local up`.

The generated `setup` service command is therefore derived from the pinned image tag:
- If the tag of `ESDIAG_IMAGE` is a release version lower than 0.18.0, emit `command: ["setup"]`.
- Otherwise, emit `command: ["setup", "env"]`. This covers 0.18.0 and later, their `-SNAPSHOT` builds, and non-version custom tags from `--esdiag-image` / `ESDIAG_IMAGE_TAG`.

Each launcher owns one helper for this (`setup_command_for_image` in Rust, `setup_command_args` in Bash), and both are tested against the same version table to keep parity. After `esdiag local upgrade` pins the new image, the next compose write switches to `setup env`.

Core mode in `bin/esdiag-local` runs `"$binary" setup env`. `native_esdiag` already requires a binary matching the launcher's `ESDIAG_VERSION`, so no version check is needed there. `src/local.rs` core mode calls the library in process.

**Trial on the local stack: on by default for a new cluster.** A local stack is often someone's first look at the Agent Builder features, so it needs an Enterprise-level license without extra steps. Manual `esdiag setup` commands assume an already-licensed cluster, which is why they stay opt-in. The local stack is the only place ESDiag starts a trial without being asked.

`esdiag local` and `esdiag-local` accept `--activate-trial=true|false` on `up`, `setup`, and `upgrade`, in the existing `--copy-password=true|false` style. `ESDIAG_SETUP_ACTIVATE_TRIAL` is honored when the flag is absent. With neither, the policy is **auto**: start a trial only if the local cluster has never had one.

"First run" is determined by asking Elasticsearch, not by inspecting launcher state:
- Before the setup stage, the launcher calls `GET _license/trial_status` with the generated `elastic` credentials. Auto resolves to `true` when `eligible_to_start_trial` is `true`, and to `false` otherwise.
- A cluster can start a trial only once. So auto activates on the first successful setup of a new stack, including an `up` that is retried after a failed first setup, and never again for those volumes. That covers later `up`, `setup`, `upgrade`, and runs after the 30-day trial has expired.
- `local reset` deletes the volumes. The next `up` creates a new cluster that is eligible again, which is the expected fresh-start behavior.
- `--activate-trial=false` (or the env var set to `false`) skips activation even on the first run. `--activate-trial=true` asks for activation on any run, and is a no-op when the license is already active.

The launcher resolves auto to a concrete boolean and hands only that to setup. `esdiag setup env` itself stays strictly opt-in. The resolved value is passed per run and never written to the state `.env` or `compose.yml`, since the cluster's trial status is the record.
- Full mode: `compose --profile setup run --rm --no-deps -e ESDIAG_SETUP_ACTIVATE_TRIAL=<resolved> setup`.
- Core mode in `bin/esdiag-local`: `ESDIAG_SETUP_ACTIVATE_TRIAL=<resolved> "$binary" setup env`.
- `src/local.rs` resolves auto with its own `reqwest` request to `trial_status`, because full mode must work in builds without the `setup` feature. Core mode then calls the in-process pipeline with the resulting `activate_trial` boolean.
- Both launchers share the eligibility check and its test cases (eligible, not eligible, request failure) to keep parity. If the `trial_status` request fails, auto resolves to `false`, and setup reports the usual Kibana-skipped warning with the opt-in command.
- A pre-0.18 pinned image ignores the variable and keeps its old always-activate behavior. On a local stack that matches the new default, so it's harmless until `local upgrade`.

`esdiag init`, when it starts a new local core stack, relies on this default and asks no trial question.

### 7. Feature gating

| Item | Gate |
|---|---|
| `Commands::Setup`, `SetupCommands::Completions` | none |
| `Elasticsearch`, `Kibana`, `Output`, `Env`, `Context` | `setup` |
| `Skills` | `agent` |

`src/lib.rs` changes from `#[cfg(feature = "setup")] pub mod setup;` to an ungated `pub mod setup;`, and each submodule carries its own gate. `command_owns_stdout` matches `SetupCommands::Completions { print: true, .. }`.

### 8. Module layout

```text
src/setup/
  mod.rs            # public re-exports
  install.rs        # SetupReport, asset installation entry points
  pipeline.rs       # shared stage pipeline, KibanaTarget, StackTargets
  elasticsearch.rs  # templates, pipelines, rollover, serverless (from setup.rs)
  license.rs        # agent_builder_license_status, activate_trial_license
  kibana.rs         # Kibana asset bundle and Agent Builder assets (from setup.rs)
  env.rs            # environment + .env resolution
  context.rs        # elasticrc resolution
  skills.rs         # from src/agent/skills.rs
  completions.rs    # from src/completions.rs
  serverless_tests.rs
```

Saved-output resolution for `setup output`, `setup elasticsearch`, and `setup kibana` lives in `OutputDeployment::resolve_saved` (`src/data/output_deployment.rs`), next to the resolver `process` and `serve` use, rather than in a separate `src/setup/output.rs`.

`src/agent/` keeps `builder.rs` for `agent ask`. Bundled asset content stays at `assets/elasticsearch/` and `assets/kibana/`, consumed by `EmbeddedAssets` and by `build.rs`'s `kibana-assets.zip`.

### 9. Structured outcomes

Outcome kinds and fields are unchanged (`setup_completed`, `agent_skills`, `completion_installed`, and the `setup_failed` category), so result parsers keep working.

## Integration Inventory

Each row is a place that names a renamed command, embeds its behavior, or verifies it.

### Rust sources

| Location | Change |
|---|---|
| `src/main.rs` `Commands::{Setup,Agent,Completions}`, `CompletionsCommands`, `AgentCommands::Skills`, `AgentSkillTarget` | `Commands::Setup { command: SetupCommands }`; remove `AgentCommands::Skills` and `Commands::Completions` |
| `src/main.rs` dispatch for `Setup` (~L1738) and `Completions` (~L1794) | move bodies into `src/setup/*` entry points |
| `src/main.rs` `command_owns_stdout` (~L744) | match `setup completions --print` |
| `src/main.rs` `run_optional_agent_skill_stage` `recovery_command` (~L2936) | `esdiag setup skills [claude|codex|opencode]...` |
| `src/main.rs` init asset stage (~L2538-2556) | separate default-no trial prompt when an existing output's license is inactive; no prompt when starting a new local stack (local auto default applies); "assets were not installed" hint → `esdiag setup output <output>` |
| `src/main.rs` bare `setup` handler (~L1752) | removed; its unconditional `ensure_agent_builder_license` becomes the opt-in license stage |
| `src/server/web_onboarding.rs` `install_output_assets_stage` (~L776-793) | unconditional trial → opt-in from the form; licensing warning surfaced |
| `templates/welcome.html` asset-install step | unchecked trial checkbox |
| `src/setup.rs` `ensure_agent_builder_license` (~L945) | split into `agent_builder_license_status` + `activate_trial_license`; update `src/setup/serverless_tests.rs` L271, `tests/serverless_setup_tests.rs` L412, `tests/elasticsearch_feedback_tests.rs` L16 |
| `src/main.rs` unit tests (~L4183) | new parse tests; removed spellings fail |
| `src/lib.rs` | ungated `setup` module, gated submodules |
| `src/setup.rs`, `src/setup/serverless_tests.rs` | split per Decision 8 |
| `src/completions.rs`, `src/agent/{skills,mod}.rs`, `src/embeds.rs` | move modules; update doc comments |
| `src/data/output_deployment.rs` | expose `from_persisted` / `from_environment` (or an explicit-source resolver) for `setup` |
| `src/local.rs` compose template (~L585), compose tests (~L1393-1433) | version-aware `setup` command (Decision 6) + tests |
| `src/local.rs` `LocalOptions::parse` (~L130), `native_setup` (~L658), `setup_mode` (~L639) | `--activate-trial=true|false` + env fallback, auto default resolved by a read-only `GET _license/trial_status`; pass the resolved value into the in-process pipeline (core) and `-e ESDIAG_SETUP_ACTIVATE_TRIAL` on `compose run` (full); help text (~L96) |
| `src/server/{index,keystore,mod,web_onboarding}.rs`, `src/client/kibana.rs` | no behavior change; verify `cfg` after gating change |
| `src/cli_output.rs` | no change |
| `Cargo.toml` | add optional `elasticrc` (path) and `dotenvy`; `setup = ["dep:elasticrc", "dep:dotenvy"]` |

### Scripts, containers, and launchers

| Location | Change |
|---|---|
| `bin/esdiag-local` `write_compose` L323-331 (`command: ["setup"]`) | version-aware `setup_command_args` (Decision 6) |
| `bin/esdiag-local` `run_setup` L562-577 | resolve auto with `curl … /_license/trial_status` (jq with sed fallback, as `configure_security` does); core: `ESDIAG_SETUP_ACTIVATE_TRIAL=<v> "$binary" setup env`; full: `-e ESDIAG_SETUP_ACTIVATE_TRIAL=<v>` on `compose run` |
| `bin/esdiag-local` option parsing (~L1131-1145), usage (~L66-90) | `--activate-trial=true|false`, `ESDIAG_SETUP_ACTIVATE_TRIAL` fallback, auto default documented |
| `bin/esdiag-control` | rejects unknown options, so parse and validate `--activate-trial=true|false` and forward it to the launcher |
| `docker/Dockerfile` `ENTRYPOINT` | unchanged; callers pass `setup env` |
| `docker/docker-compose{,-insecure}.yml` | unchanged (`esdiag-kibana-setup` is a password bootstrap) |
| `bin/esdiag-lite.{sh,ps1}` | unchanged |
| `bin/sync-plugin-skill.sh` | rerun after skill edits |

### Tests

| Location | Change |
|---|---|
| `tests/serverless_setup_tests.rs` L38 | `setup kibana` → `setup kibana kibana` |
| `tests/serverless_setup_tests.rs` L321 | bare `setup` with env → `setup env` |
| `tests/serverless_setup_tests.rs`, `tests/elasticsearch_feedback_tests.rs` library calls | unchanged via `esdiag::setup::*` |
| `tests/esdiag-local.sh` fake `esdiag` (L50) | log full args; assert core mode runs `setup env` (L133, L173) |
| `tests/esdiag-local.sh` compose assertions (~L107-121) | assert `command: ["setup", "env"]` for current image and `["setup"]` for a 0.17.x-pinned state |
| `tests/esdiag-local.sh` trial policy | fake `curl` serves `trial_status`: eligible → setup receives `true`; not eligible → `false`; request failure → `false`; `--activate-trial=false` and the env var override eligibility; `--activate-trial=true` passes `true`; the value never appears in `.env` or `compose.yml` |
| `tests/bin/esdiag-control.sh` | add forwarding assertion for `--activate-trial=true` |
| `tests/plugin.sh` L54 | assert `esdiag setup skills` |
| `src/completions.rs` tests | move; assert scripts contain `setup` subcommands and no top-level `completions` |
| new | license matrix: active / inactive × opted in / not × serverless, asserting no `start_trial` without opt-in, plus Kibana-skipped `partial` outcome and warning text; `setup output` (default, named, no viewer); `setup env` (env only, `--env-file` only, env overrides file, partial, none declared, auth-combination errors); `setup context` (fixtures + mock servers, partial, unknown, no secret leakage); trial-license matrix; wrong-`app` rejection |

### GitHub Actions

| Workflow | Effect |
|---|---|
| `openspec-archived.yml` | Fails the PR while `openspec/changes/consolidate-setup-subcommands/` is unarchived; archive before merge. |
| `elasticrc.yml` | Runs because `Cargo.toml`/`Cargo.lock` change. |
| `release-esdiag-local.yml` | Renders `bin/esdiag-local`, runs `version` and `shellcheck`; must stay green with the new helper. |
| `esdiag-lite.yml`, `desktop-artifacts.yml` | Unaffected. |
| (none) | No workflow runs `cargo test`, `cargo clippy`, `tests/plugin.sh`, or `tests/esdiag-local.sh`; run locally and record in the PR. |

### Agent skill and plugin

| Location | Change |
|---|---|
| `.agents/skills/esdiag/references/esdiag-cli.md` L15, L122-123 | `esdiag setup skills`; document `setup elasticsearch|kibana|env|context` |
| `plugin/skills/esdiag/references/*` | regenerate with `bin/sync-plugin-skill.sh` |
| `plugin/README.md` L5, L30, L41 | `esdiag setup skills`; asset setup via explicit subcommands |

### Documentation (also embedded via `DocsAssets`)

| Location | Change |
|---|---|
| `docs/command-line.md` command table, stdout note, `## setup`, `## agent`, `## completions` | one `setup` section with per-subcommand blocks; `agent` documents `ask` only; remove `## completions` |
| `docs/setup/installation.md` L49, L131 | `esdiag setup completions zsh`, `esdiag setup skills` |
| `docs/setup/agent-skills.md` L27, L34, L41 | `esdiag setup skills [claude codex]` |
| `docs/setup/remote-cluster.md` L100-128, `docs/setup/serverless.md` L57-82 | `setup output <name>`, `setup elasticsearch <host>`, `setup kibana <host>`, `docker run … setup env`; document `--activate-trial` |
| `docs/setup/local-stack.md` L160, `docs/bin/esdiag-local.md` | a new local stack starts a trial automatically; `--activate-trial=false` / `ESDIAG_SETUP_ACTIVATE_TRIAL=false` to skip it |
| `docs/setup/shared-service.md` L50, `docs/setup/index.md` L57 | explicit subcommands |
| `docs/reference/serverless-assets.md` L186, L232-233 | `setup env` / `setup elasticsearch esdiag-less`, `setup kibana esdiag-less-kb` |
| `docs/repository/organization.md` L37, L45, `assets/readme.md` | `src/setup/` layout and subcommands |
| `example.env` | note `esdiag setup env --env-file` usage; commented `ESDIAG_SETUP_ACTIVATE_TRIAL=false` |
| `docs/adr/0014-…` | historical; unchanged |
| `CHANGELOG.md` `[Unreleased]` | replace the #430 `completions` entry; add Breaking/Added entries |

### Specs

`standalone-local-stack` receives deltas for the one-shot setup command and trial opt-in, and `embedded-local-stack-launcher` for trial-option parity. `kibana-diagnostic-collection` refers to "`esdiag setup`" generically and stays valid.

## Risks / Trade-offs

- **Old scripts break loudly.** That is intended for pre-1.0. The exception is `setup kibana` / `setup elasticsearch` against a saved host with that exact name, which retargets to the persisted output. The CHANGELOG documents it.
- **Manual setup on an unlicensed cluster stops short of Kibana.** A remote output on a basic license finishes with Elasticsearch assets only, plus a warning, until the user passes `--activate-trial` or installs a license. That is the intended trade for never changing a working cluster's license implicitly.
- **The local auto policy depends on `GET _license/trial_status`.** If that request fails, the first run proceeds without a trial and prints the opt-in command, instead of guessing.
- **Two launchers must stay in parity on the version rule.** Mitigated by a shared test table in both test suites.
- **The embedded skill digest changes**, so skills installed by an older binary report `updated`.
- **`elasticrc` resolvers can run programs (`cmd`, `pass`).** This is the crate's existing contract, and only the selected context's services resolve.

## Migration Plan

1. Ship in 0.18.0 with Breaking CHANGELOG entries listing each old → new spelling.
2. Existing local stacks keep working until `esdiag local upgrade`, after which their compose file uses `setup env`.

Rollback: revert the PR. No persisted state (hosts, keystore, config, installed skills, local `.env`) changes shape.

## Open Questions

None.
