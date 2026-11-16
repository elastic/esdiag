# cli-setup-command Specification

## Purpose

Define the `esdiag setup` command family that configures external systems for ESDiag: Elasticsearch and Kibana assets from saved hosts, saved outputs, environment variables, or Elastic CLI contexts, plus coding-agent skills and shell completions, with trial license activation only on explicit opt-in.

## Requirements

### Requirement: Setup Requires An Explicit Target
The CLI SHALL provide `esdiag setup` as the single command family for configuring external systems for ESDiag. It SHALL expose `elasticsearch`, `kibana`, `output`, `env`, `context`, `skills`, and `completions` subcommands, and SHALL require one of them. `esdiag setup` with no subcommand SHALL print help and exit non-zero without contacting any service. The CLI MUST NOT accept a positional host directly on `setup`.

#### Scenario: Bare setup prints help
- **WHEN** the user runs `esdiag setup`
- **THEN** the help lists `elasticsearch`, `kibana`, `output`, `env`, `context`, `skills`, and `completions`
- **AND** the command exits non-zero
- **AND** no network request is sent

#### Scenario: Removed positional host form
- **GIVEN** a saved host `diagnostics-output`
- **WHEN** the user runs `esdiag setup diagnostics-output`
- **THEN** the command fails as a usage error
- **AND** no network request is sent

#### Scenario: Removed command spellings
- **WHEN** the user runs `esdiag agent skills` or `esdiag completions zsh`
- **THEN** the command fails as a usage error

### Requirement: Setup Elasticsearch Installs Only Elasticsearch Assets
`esdiag setup elasticsearch [HOST]` SHALL install Elasticsearch assets into `HOST`, or into the persisted default output's Elasticsearch when `HOST` is omitted. `HOST` MUST be a saved host whose `app` is Elasticsearch. The command MUST NOT read `ESDIAG_OUTPUT_*`, MUST NOT install Kibana assets, and MUST NOT start a trial license.

#### Scenario: Saved Elasticsearch host
- **GIVEN** a saved host `diagnostics-output` with `app: elasticsearch`
- **WHEN** the user runs `esdiag setup elasticsearch diagnostics-output`
- **THEN** only Elasticsearch assets are installed
- **AND** the `setup_completed` outcome lists one target

#### Scenario: Persisted default output
- **GIVEN** the application config names default output `diagnostics-output`
- **AND** `ESDIAG_OUTPUT_URL` points at a different cluster
- **WHEN** the user runs `esdiag setup elasticsearch`
- **THEN** assets are installed into `diagnostics-output`
- **AND** the cluster named by `ESDIAG_OUTPUT_URL` receives no request

#### Scenario: Host has the wrong application
- **GIVEN** a saved host `diagnostics-kb` with `app: kibana`
- **WHEN** the user runs `esdiag setup elasticsearch diagnostics-kb`
- **THEN** the command fails before any request is sent
- **AND** the error suggests `esdiag setup kibana diagnostics-kb`

#### Scenario: Inactive Agent Builder license is left alone
- **GIVEN** the target cluster has a basic license
- **WHEN** the user runs `esdiag setup elasticsearch diagnostics-output`
- **THEN** no `_license/start_trial` request is sent

### Requirement: Setup Kibana Installs Only Kibana Assets
`esdiag setup kibana [HOST]` SHALL install Kibana assets into `HOST`, or into the persisted default output's linked Kibana viewer when `HOST` is omitted. `HOST` MUST be a saved host whose `app` is Kibana. The command SHALL install into the space `ESDIAG_KIBANA_SPACE` selects (default `esdiag`), the same space `process`, `serve`, and `agent ask` link to; a `/s/<space>` path in the Kibana URL MUST NOT select the space. It MUST NOT send Elasticsearch asset requests.

#### Scenario: Saved Kibana host in a space
- **GIVEN** a saved Kibana host `diagnostics-kb`
- **AND** `ESDIAG_KIBANA_SPACE=esdiag`
- **WHEN** the user runs `esdiag setup kibana diagnostics-kb`
- **THEN** Kibana assets are installed into the `esdiag` space

#### Scenario: Saved host named like the subcommand
- **GIVEN** a saved Kibana host named `kibana`
- **WHEN** the user runs `esdiag setup kibana kibana`
- **THEN** Kibana assets are installed into the saved host `kibana`

#### Scenario: Inactive license does not block explicit Kibana setup
- **GIVEN** a saved Kibana host `diagnostics-kb`
- **WHEN** the user runs `esdiag setup kibana diagnostics-kb`
- **THEN** no Elasticsearch `_license` request is sent
- **AND** any Kibana licensing error is reported as returned by Kibana

#### Scenario: Persisted output has no viewer
- **GIVEN** the persisted default output has no linked Kibana viewer
- **WHEN** the user runs `esdiag setup kibana`
- **THEN** the command fails with a `setup_failed` outcome before any request is sent

### Requirement: Setup Output Uses A Saved Output Deployment
`esdiag setup output [NAME] [--activate-trial]` SHALL resolve the saved Elasticsearch host `NAME`, or the persisted default output when `NAME` is omitted. The host MUST have role `send`. It SHALL set up that Elasticsearch and its linked Kibana viewer through the combined pipeline. The command MUST NOT read `ESDIAG_OUTPUT_*` or `ESDIAG_KIBANA_URL`. When the output has no linked viewer, it SHALL set up Elasticsearch only and warn that no viewer is linked.

#### Scenario: Upgrade re-run on a working cluster
- **GIVEN** the persisted default output `diagnostics-output` has a linked viewer and an active Enterprise license
- **WHEN** the user runs `esdiag setup output` after upgrading ESDiag
- **THEN** Elasticsearch and Kibana assets are updated
- **AND** no `_license/start_trial` request is sent

#### Scenario: Named output
- **GIVEN** saved outputs `prod-output` and `lab-output`, with `lab-output` as the default
- **WHEN** the user runs `esdiag setup output prod-output`
- **THEN** assets are installed into `prod-output` and its viewer only

#### Scenario: Environment does not redirect a saved output
- **GIVEN** `ESDIAG_OUTPUT_URL` points at a different cluster
- **WHEN** the user runs `esdiag setup output`
- **THEN** the cluster named by `ESDIAG_OUTPUT_URL` receives no request

#### Scenario: Name is not an output host
- **GIVEN** a saved host `diagnostics-kb` with `app: kibana`
- **WHEN** the user runs `esdiag setup output diagnostics-kb`
- **THEN** the command fails before any request is sent

### Requirement: Setup Env Uses Environment Variables And Optional Env Files
`esdiag setup env [--env-file <PATH>] [--activate-trial]` SHALL read `ESDIAG_OUTPUT_URL`, `ESDIAG_OUTPUT_APIKEY`, `ESDIAG_OUTPUT_USERNAME`, `ESDIAG_OUTPUT_PASSWORD`, `ESDIAG_KIBANA_URL`, `ESDIAG_KIBANA_SPACE`, and `ESDIAG_SETUP_ACTIVATE_TRIAL` from the process environment, and, when `--env-file` is given, from that file. Process environment values SHALL take precedence over file values. The command MUST NOT read any `.env` file implicitly, MUST NOT modify the process environment or pass file values to child processes, and SHALL ignore unrelated keys in the file. It SHALL set up Elasticsearch when `ESDIAG_OUTPUT_URL` is declared, and Kibana when `ESDIAG_KIBANA_URL` is declared. It SHALL apply the existing `ESDIAG_OUTPUT_*` authentication-combination rules.

#### Scenario: Container one-shot setup
- **GIVEN** `ESDIAG_OUTPUT_URL`, `ESDIAG_OUTPUT_APIKEY`, and `ESDIAG_KIBANA_URL` are set
- **AND** no saved hosts exist
- **AND** the cluster's Agent Builder license is active
- **WHEN** the container image runs `esdiag setup env`
- **THEN** Elasticsearch assets, then Kibana assets, are installed
- **AND** the `setup_completed` outcome lists both targets

#### Scenario: Env file opts into the trial
- **GIVEN** `deploy.env` declares both services and `ESDIAG_SETUP_ACTIVATE_TRIAL=true`
- **AND** the cluster has a basic license
- **WHEN** the user runs `esdiag setup env --env-file deploy.env`
- **THEN** a trial license is started before the Kibana stage

#### Scenario: Env file supplies the deployment
- **GIVEN** a file `deploy.env` containing `ESDIAG_OUTPUT_URL`, `ESDIAG_OUTPUT_APIKEY`, and `ESDIAG_KIBANA_URL`
- **AND** none of those variables are set in the process environment
- **WHEN** the user runs `esdiag setup env --env-file deploy.env`
- **THEN** assets are installed into the deployment declared by the file

#### Scenario: Process environment overrides the file
- **GIVEN** `deploy.env` sets `ESDIAG_OUTPUT_URL=https://file.example:9200`
- **AND** the process environment sets `ESDIAG_OUTPUT_URL=https://env.example:9200`
- **WHEN** the user runs `esdiag setup env --env-file deploy.env`
- **THEN** Elasticsearch assets are installed into `https://env.example:9200`

#### Scenario: No implicit env file
- **GIVEN** the working directory contains a `.env` declaring `ESDIAG_OUTPUT_URL`
- **AND** the process environment declares neither `ESDIAG_OUTPUT_URL` nor `ESDIAG_KIBANA_URL`
- **WHEN** the user runs `esdiag setup env`
- **THEN** the command fails before any request is sent
- **AND** the error names `ESDIAG_OUTPUT_URL`, `ESDIAG_KIBANA_URL`, and `--env-file`

#### Scenario: Elasticsearch only
- **GIVEN** only `ESDIAG_OUTPUT_URL` and `ESDIAG_OUTPUT_APIKEY` are declared
- **WHEN** the user runs `esdiag setup env`
- **THEN** only Elasticsearch assets are installed
- **AND** no trial license request is sent
- **AND** the outcome warns that Kibana setup was skipped because `ESDIAG_KIBANA_URL` is not declared

#### Scenario: Conflicting authentication
- **GIVEN** `ESDIAG_OUTPUT_APIKEY` and `ESDIAG_OUTPUT_USERNAME` are both declared
- **WHEN** the user runs `esdiag setup env`
- **THEN** the command fails before any request is sent

### Requirement: Setup Context Uses An Elastic CLI Context
`esdiag setup context [NAME] [--config <PATH>] [--activate-trial]` SHALL load the Elastic CLI configuration from `--config`, then `ELASTIC_CLI_CONFIG_FILE`, then home discovery. It SHALL select the named context, or the current context when `NAME` is omitted, and set up each of the context's `elasticsearch` and `kibana` services that is defined. Credentials SHALL be resolved in memory and MUST NOT be written to `hosts.yml`, the keystore, the application config, or any outcome.

#### Scenario: Full context
- **GIVEN** an `.elasticrc.yml` whose current context `prod` defines `elasticsearch` and `kibana` with API key auth
- **AND** `prod` has an active Agent Builder license
- **WHEN** the user runs `esdiag setup context`
- **THEN** Elasticsearch assets, then Kibana assets, are installed into `prod`
- **AND** `hosts.yml` and the keystore are unchanged

#### Scenario: Partial context
- **GIVEN** context `es-only` defines only `elasticsearch`
- **WHEN** the user runs `esdiag setup context es-only`
- **THEN** only Elasticsearch assets are installed
- **AND** no trial license request is sent
- **AND** the outcome warns that context `es-only` defines no `kibana` service

#### Scenario: Resolver authentication
- **GIVEN** context `staging` resolves its Elasticsearch API key through an `env` resolver
- **WHEN** the user runs `esdiag setup context staging`
- **THEN** the resolved key authenticates the Elasticsearch requests
- **AND** the `setup_completed` outcome contains no secret value

#### Scenario: Context defines neither service
- **GIVEN** context `cloud-only` defines only `cloud`
- **WHEN** the user runs `esdiag setup context cloud-only`
- **THEN** the command fails before any request is sent, naming context `cloud-only`

#### Scenario: Unknown context
- **WHEN** the user runs `esdiag setup context missing`
- **THEN** the command fails with a `setup_failed` outcome listing the available context names

### Requirement: Trial License Activation Is Opt-In
No `esdiag setup` subcommand SHALL start an Elasticsearch trial license unless the caller explicitly opts in, because manual setup assumes an already-licensed cluster. The local-stack launchers, which may pass a resolved opt-in for a new local cluster, are specified separately. Only `setup output`, `setup env`, and `setup context` SHALL accept `--activate-trial`, which can also be set with `ESDIAG_SETUP_ACTIVATE_TRIAL=true`. When one of these commands sets up both Elasticsearch and Kibana, it SHALL read the Agent Builder license status (read-only, skipped on serverless) between the two stages.
- If the license is active or managed, it SHALL continue to Kibana.
- If the license is inactive and the caller opted in, it SHALL start and verify a trial, then continue.
- If the license is inactive and the caller did not opt in, it SHALL skip the Kibana stage, report outcome `partial`, and add a warning naming `--activate-trial` and `ESDIAG_SETUP_ACTIVATE_TRIAL=true`.

`setup elasticsearch`, `setup kibana`, and single-service setups MUST NOT read or change the license. A source that deliberately declares only one service SHALL NOT by itself make the outcome `partial`.

#### Scenario: Re-run without opt-in on a basic license
- **GIVEN** an output whose cluster has an active basic license
- **WHEN** the user runs `esdiag setup output`
- **THEN** Elasticsearch assets are installed
- **AND** no `_license/start_trial` request is sent
- **AND** the Kibana stage is skipped
- **AND** the outcome is `partial` with a warning naming `--activate-trial` and `ESDIAG_SETUP_ACTIVATE_TRIAL=true`

#### Scenario: Explicit opt-in on a basic license
- **GIVEN** an output whose cluster has an active basic license
- **WHEN** the user runs `esdiag setup output --activate-trial`
- **THEN** a trial license is started and verified before the Kibana stage
- **AND** Kibana assets are installed

#### Scenario: Environment opt-in
- **GIVEN** `ESDIAG_SETUP_ACTIVATE_TRIAL=true`
- **AND** a context whose cluster has a basic license
- **WHEN** the user runs `esdiag setup context prod`
- **THEN** a trial license is started before the Kibana stage

#### Scenario: Explicit false keeps the license unchanged
- **GIVEN** `ESDIAG_SETUP_ACTIVATE_TRIAL=false`
- **WHEN** the user runs `esdiag setup env` against a basic-license cluster
- **THEN** no `_license/start_trial` request is sent

#### Scenario: Serverless
- **GIVEN** a serverless Elasticsearch project
- **WHEN** the user runs `esdiag setup output --activate-trial`
- **THEN** no `/_license` or `/_license/start_trial` request is sent
- **AND** the Kibana stage runs

#### Scenario: Single-service commands reject the flag
- **WHEN** the user runs `esdiag setup elasticsearch --activate-trial`
- **THEN** the command fails as a usage error

#### Scenario: Kibana-only env
- **GIVEN** only `ESDIAG_KIBANA_URL` and `ESDIAG_OUTPUT_APIKEY` are declared
- **WHEN** the user runs `esdiag setup env`
- **THEN** only Kibana assets are installed
- **AND** no Elasticsearch request is sent
- **AND** the outcome is `complete` when every Kibana asset installs

### Requirement: Setup Subcommands Follow Cargo Features
The `setup` command SHALL be available in every build. `setup completions` SHALL be available without optional features. `setup elasticsearch`, `setup kibana`, `setup output`, `setup env`, and `setup context` SHALL require the `setup` feature, and `setup skills` SHALL require the `agent` feature. The library SHALL keep `esdiag::setup::{assets, assets_report, asset_statuses, assets_installed, SetupReport}` available under the `setup` feature. It SHALL expose license handling only as the read-only `agent_builder_license_status` and the explicit `activate_trial_license`.

#### Scenario: Minimal build
- **WHEN** ESDiag is built with `--no-default-features`
- **THEN** `esdiag setup completions bash --print` works
- **AND** `esdiag setup env` and `esdiag setup skills` are rejected as unknown subcommands

### Requirement: Generated Completions Reflect The Setup Tree
Generated completion scripts SHALL complete every available `setup` subcommand and MUST NOT complete removed spellings.

#### Scenario: Zsh completions
- **WHEN** the user runs `esdiag setup completions zsh --print`
- **THEN** the script completes `setup elasticsearch`, `setup kibana`, `setup output`, `setup env`, `setup context`, `setup skills`, and `setup completions`
- **AND** does not complete `agent skills` or a top-level `completions` command
