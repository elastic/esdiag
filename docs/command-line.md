---
type: Reference
title: Command-line interface reference
description: Commands, inputs, outputs, and runtime settings for esdiag.
tags: [cli, reference]
---

# Command-line interface reference

This is a reference, not a copy of generated help. Your binary has the exact
flags for its version:

```sh
esdiag --help
esdiag <command> --help
```

The same execution layer is available through a smaller
`elastic diag <args...>` command profile when ESDiag is registered as an
experimental Elastic CLI extension.

## Commands

| Command | What it does |
|---|---|
| `collect` | Collects an archive from a saved source. |
| `serve` | Starts the diagnostic submission web service. |
| `host` | Saves, tests, and removes host definitions. |
| `init` | Runs interactive user setup. |
| `local` | Manages a local Elasticsearch, Kibana, and ESDiag stack. |
| `keystore` | Manages encrypted credentials. |
| `process` | Processes an archive, directory, saved host, or Elastic Upload Service URL. |
| `send` | Sends a raw archive to Elastic Upload Service. |
| `setup` | Installs ESDiag assets, agent skills, and shell completions. |
| `agent` | Asks Kibana Agent Builder about diagnostics. |
| `job` | Runs, lists, and deletes saved jobs. |

## Global behavior

`--debug` writes debug logs. `--format yaml|json` selects the result format for
finite commands. YAML is the default.

Finite commands write one result to stdout. Short progress messages, warnings,
and errors go to stderr. Each command's full log, including container runtime
output from `esdiag local`, goes to `~/.esdiag/last_run/esdiag.log`, which is
replaced on the next run. `--debug` adds debug detail to the log and the
terminal. `esdiag serve` logs to stderr instead.
Processing results include `outcome`, `documents_failed`, and `indexing_failures`
with destination data stream names and rejection counts, including documents
redirected to a failure store. `output` identifies the resolved
destination, including a saved default deployment. A `setup_completed` result
has an `outcome` of `complete` or `partial`; partial mapping updates include
`failed_indices` and recovery advice in `warnings`. Rejected setup assets also
appear in `warnings`; they do not prevent a combined setup command from
attempting Kibana installation. A Kibana stage skipped for licensing also makes
the outcome `partial`. A service the source does not declare is noted in
`warnings` without making the outcome `partial`.

Process results use the Kibana viewer that belongs to the output deployment:
the Elastic CLI context's Kibana service, `ESDIAG_KIBANA_URL` for an
`ESDIAG_OUTPUT_URL` output, or the saved host's linked viewer. An output with
no viewer of its own omits `kibana_url`; `ESDIAG_KIBANA_URL` is never attached
to a different output, and `kibana_url` never includes URL credentials. See
[Serverless asset compatibility](reference/serverless-assets.md#indexing-failures-and-recovery)
for rollover and failure-store recovery steps.

When a command fails after it starts, stdout contains a `command_failed` result
and the command exits non-zero.

`process ... -` writes only NDJSON documents to stdout. It never appends a YAML
or JSON result. `esdiag setup completions <shell> --print` writes only a shell
script to stdout. `serve` writes one readiness result after it binds unless its output
owns stdout.

## Local state and environment

By default, ESDiag stores user state in `~/.esdiag`:

| File | Contents |
|---|---|
| `hosts.yml` | Saved endpoints and secret references. |
| `secrets.yml` | Encrypted keystore. |
| `esdiag.yml` | Non-secret user defaults. |
| `settings.yml` | Saved UI settings. |
| `jobs.yml` | Saved jobs. |
| `last_run/` | The last command's `esdiag.log` and debug files. |

`ESDIAG_HOSTS` and `ESDIAG_KEYSTORE` override the paths to `hosts.yml` and
`secrets.yml`.

These environment variables define an output when `process` or `serve` does
not receive one, and the deployment for `esdiag setup env`:

- `ESDIAG_OUTPUT_URL`
- `ESDIAG_OUTPUT_APIKEY`
- `ESDIAG_OUTPUT_USERNAME`
- `ESDIAG_OUTPUT_PASSWORD`
- `ESDIAG_KIBANA_URL`

ESDiag uses one complete output definition. It does not combine endpoints or
credentials from command arguments, environment variables, and `esdiag.yml`.

`LOG_LEVEL` sets the log level for the run log and the terminal. Without it, the
run log records `info` and the terminal shows warnings and errors.
`ESDIAG_KEYSTORE_PASSWORD` supplies a keystore password for non-interactive use.

### Elastic CLI extension

Install the `esdiag` binary, then register this checkout under the explicit
short name `diag`:

```sh
cargo install --path .
elastic extension create diag --path "$PWD"
elastic diag help
```

The native binary selects its Elastic CLI profile when invoked as
`elastic-diag`; `ESDIAG_ELASTIC_CLI=1` remains available for the repository's
development wrapper. Both profiles use the same Rust execution layer. The
extension profile exposes `collect`, `process`, `send`, `setup`, `job`,
`output`, `help`, and `version`. Host, keystore, local-stack, server, and agent
administration remain under `esdiag`.

Use `elastic diag help [COMMAND]` for extension help. Current Elastic CLI
releases consume `--help` before dispatching an extension. Self-contained
publication under the required `diag` name is specified separately; installing
`elastic/esdiag` directly currently derives the wrong command name.

The extension accepts the active context values supplied by Elastic CLI:

- `ELASTIC_ES_URL`, `ELASTIC_ES_API_KEY`, `ELASTIC_ES_USERNAME`, and
  `ELASTIC_ES_PASSWORD`
- `ELASTIC_KIBANA_URL`, `ELASTIC_KIBANA_API_KEY`, `ELASTIC_KIBANA_USERNAME`,
  and `ELASTIC_KIBANA_PASSWORD`
- `ELASTIC_CLOUD_URL` and `ELASTIC_CLOUD_API_KEY`

Arguments that resolve remote targets also accept these references:

| Reference | Target |
|---|---|
| `.es`, `.elasticsearch` | Elasticsearch in the active context. |
| `.kb`, `.kibana` | Kibana in the active context. |
| `.context.es`, `.context.elasticsearch` | Elasticsearch in a named context. |
| `.context.kb`, `.context.kibana` | Kibana in a named context. |
| `.cloud/<deployment-id>/es` | Elasticsearch through the active context's Cloud admin API. |
| `.context.cloud/<deployment-id>/es` | Elasticsearch through a named context's Cloud admin API. |

Named references are loaded read-only from `ELASTIC_CLI_CONFIG_FILE` or the
supported `.elasticrc` files in the user's home directory. The rightmost
segment is the service, so `.prod.us-west.es` selects context `prod.us-west`.
Use an explicit filesystem prefix such as `./.es` for a local hidden path.
Cloud references require both the deployment ID and application. The Cloud
management endpoint is never treated as an application endpoint, and the
proxy currently accepts only `es` or `elasticsearch`.

The extension never guesses a process input. Select an application explicitly:

```sh
elastic diag process .es
elastic diag process .customer.kb .monitoring.es
elastic diag process .prod.cloud/415715723947/es .monitoring.es
```

Configure a stable named output context while changing input contexts between
commands:

```sh
elastic diag output set monitoring
elastic diag output show
elastic diag output clear
```

Only the context name and optional config-file path are saved in `esdiag.yml`.
Endpoints and credentials are resolved from Elastic CLI configuration when a
job runs. Saved jobs likewise retain symbolic context references.

Use `elastic diag send <archive> <upload-id>` to transmit a raw bundle to the
Elastic Upload Service.

Resolver expressions are supported for Elastic CLI config compatibility,
including environment, file, OS credential store, `pass`, and command-backed
values. Command-backed resolvers execute explicit arguments without a shell
and with bounded time and output; use them only with config files you trust.

## Output selection

`process` and `serve` accept an optional output:

| Value | Result |
|---|---|
| `-` | Write processed documents to stdout. |
| `.service` or `.context.service` | Use an Elastic CLI context target. |
| Saved host name | Send to that host. |
| Other non-empty string | Write to a local file or directory. |
| Omitted | Use the configured named Elastic CLI output context, a complete standalone `ESDIAG_OUTPUT_*` deployment, or the saved-host default in `esdiag.yml`. |

An active input context is never an implicit output. An explicit output
positional takes precedence over the configured output context.

The web workflow's `Default` output reuses the active `serve` target, including
an explicit output supplied at startup.

Save HTTP URLs as hosts before using them as outputs. A raw `http://` or
`https://` argument is a file path, not an Elasticsearch destination.

## `host`

`host add`, `update`, `remove`, `list`, and `auth` manage entries in
`hosts.yml`.

Create a saved Elasticsearch host:

```sh
esdiag host add prod-es https://es.example.com:9200 --roles send
```

`host add` and `host update` test the full definition before saving it.
`host auth <NAME>` tests an existing definition without changing it.

Roles:

- `collect` is valid for Elasticsearch, Kibana, and Logstash.
- `send` is valid for Elasticsearch.
- `view` is valid for Kibana.

If you omit roles, the host gets `collect`. ESDiag infers the app from ports
9200, 5601, and 9600. On any other port, pass `--app`.

`host add` uses the keystore entry with the same name as the host when one
exists. Use `--secret <SECRET_ID>` to reference a differently named entry.
`--apikey` or
`--user` with `--password` can supply credentials while you create or update
the host. Keep certificate validation enabled unless the endpoint owner has
approved an exception.

## `keystore`

The keystore holds credentials outside `hosts.yml`.

```sh
esdiag keystore unlock
esdiag keystore add prod-es --apikey
esdiag keystore update prod-es --apikey
esdiag keystore status
esdiag keystore lock
```

Omit the value after `--apikey` or `--password` at an interactive terminal.
ESDiag then prompts without echoing the secret. Do not pass secrets in normal
shell arguments.

`keystore password` changes the keystore password. `keystore migrate` moves
legacy plaintext host credentials into the keystore.

## `init`

`esdiag init` is an interactive command. It can create:

- A collection host and default collection job.
- A local or remote output and linked Kibana viewer.
- A default collect-and-process job.
- Output assets when you approve installation for an existing local or remote
  deployment.

It stores credentials in `secrets.yml`, not `esdiag.yml`.

Enter an email address or another diagnostic user identifier. The default is
`EMAIL` when it holds an email address, then `git config user.email`, then the
`whoami` login name.
Invalid yes/no answers, endpoint URLs, default-job host selections, and empty
or mismatched new keystore passwords prompt again. The default job requires a saved collection host name, not a URL.
Resuming displays the saved workflow, and changing it displays both choices.

Pasted keys are hidden and used to validate Elasticsearch and Kibana before
saving the output. Validation identifies the failing application and reports
authentication, TLS, DNS, connection, or response failures. You can retry the
output step without restarting onboarding. The local API key source appears
only when a usable local key is detected.

To replace a referenced API key later, run `esdiag keystore update <name> --apikey`
at a terminal. It prompts for the key without putting it in shell history.

The diagnostic user is the first question, and `init` saves it before
anything else. An incomplete workflow can then continue in the web interface:
`init` starts `esdiag serve --mode user`, opens `/welcome`, and keeps the
terminal attached until you stop the server with Ctrl+C. The web flow starts
with the saved user. The terminal and web flows persist the same user,
workflow, host, job, output, and keystore state.

When you select local processing and no stack exists, `init` can start a
binary-owned core stack. Its approval includes that new stack's required
assets; declining returns to remote output setup. After a terminal setup that
started a stack, `init` asks whether to open the web interface. The default is
no.

When you approve asset setup for an existing output whose license does not
include Agent Builder, `init` asks whether to start a 30-day trial license. The
default is no; declining installs the Elasticsearch assets and skips Kibana's
Agent Builder assets. A new local stack starts its trial without asking.

See [Set up ESDiag](setup/index.md) for the guided setup in each journey.

## `local`

`esdiag local <command>` runs the binary's Rust-owned local-stack lifecycle:

`esdiag local --help` lists the available commands. If startup fails, the
generated `.env` and `compose.yml` remain in the state directory. Use `logs`
to inspect the failure, retry `up`, or stop the stack with `down`. Include
the same `--state-dir` when using a custom directory. `secrets password`
continues to read the retained password.

During startup, Elasticsearch authentication failures are retried while its
security index initializes. If authentication still fails when readiness times
out, check the retained credentials. `local auth` reports rejected credentials
immediately. The managed native server uses the local deployment's output and
Kibana settings, overriding ambient `ESDIAG_OUTPUT_*` and `ESDIAG_KIBANA_*`
variables. If that child exits during startup, `up` reports its exit status and
log path without waiting for the readiness timeout.

```sh
esdiag local up
esdiag local status
esdiag local auth
esdiag local logs
esdiag local down
```

`up` accepts `--stack=auto|core|full`. `up`, `setup`, and `upgrade` accept
`--activate-trial=true|false`, or read `ESDIAG_SETUP_ACTIVATE_TRIAL`. Without
either, they start an Elasticsearch trial license only when the local cluster
has never had one, so a new stack gets Agent Builder and later runs leave the
license alone.

- `auto` defaults to core.
- `core` runs Elasticsearch and Kibana in containers and starts native
  `esdiag serve --mode user`.
- `full` runs the ESDiag web service in a container.

The state directory records the chosen mode. Changing modes does not move
hosts, jobs, settings, or secrets between the native user directory and the
full-mode container volume.

`esdiag local update` cannot update binary-owned lifecycle code. Update the
binary through Homebrew, Cargo, or its release archive.

`up` keeps the image versions recorded in the state directory and warns when
the binary ships newer ones. `esdiag local upgrade` moves the stack to them
after a `[y/N]` confirmation, or immediately with `--force`. It restarts a
running stack and leaves a stopped stack stopped.

See [Run a local stack](setup/local-stack.md) for local setup and
[local-stack launcher reference](bin/esdiag-local.md) for every launcher
option.

## `setup`

`esdiag setup` configures external systems for ESDiag. It requires a
subcommand; `esdiag setup` alone prints help and exits non-zero.

| Command | Configures |
|---|---|
| `setup output [NAME] [--activate-trial]` | A saved output and its linked Kibana viewer |
| `setup elasticsearch [HOST]` | Elasticsearch assets on one saved host |
| `setup kibana [HOST]` | Kibana assets on one saved host |
| `setup env [--env-file PATH] [--activate-trial]` | The deployment declared by `ESDIAG_OUTPUT_*` and `ESDIAG_KIBANA_*` |
| `setup context [NAME] [--config PATH] [--activate-trial]` | The services of an Elastic CLI `.elasticrc.yml` context |
| `setup skills [AGENT]... [--force]` | The embedded ESDiag skill in coding agents |
| `setup completions <SHELL> [--print]` | Shell completions |

Run asset setup before the first ingest and after an upgrade that changes
assets. You can rerun it safely; existing Kibana workflows and other managed
assets are updated to the bundled definitions. The credential needs permission
to install them.

### Elastic Stack assets

`setup output` uses the named saved output, or the persisted default output
when `NAME` is omitted. The host must have the `send` role. It installs
Elasticsearch assets, then Kibana assets into the linked viewer. Without a
linked viewer it installs Elasticsearch assets only and says so.

```sh
esdiag setup output diagnostics-output
```

`setup elasticsearch` and `setup kibana` install one service's assets into a
saved host whose `app` matches the subcommand. Without `HOST`, they use the
persisted default output or its viewer. A saved host may share the subcommand's
name, as in `esdiag setup kibana kibana`. Kibana assets go to the space
`ESDIAG_KIBANA_SPACE` selects (default `esdiag`), even when the saved Kibana URL
contains a `/s/<space>` path.

None of these saved-host commands read `ESDIAG_OUTPUT_*`. Use `setup env` for
an environment-declared deployment, such as a container or CI job:

```sh
esdiag setup env
esdiag setup env --env-file deploy.env
```

`setup env` reads `ESDIAG_OUTPUT_URL`, `ESDIAG_OUTPUT_APIKEY`,
`ESDIAG_OUTPUT_USERNAME`, `ESDIAG_OUTPUT_PASSWORD`, `ESDIAG_KIBANA_URL`,
`ESDIAG_KIBANA_SPACE`, and `ESDIAG_SETUP_ACTIVATE_TRIAL`. With `--env-file`,
values missing from the process environment come from that file; the process
environment wins, and other keys in the file are ignored. It never reads a
`.env` file unless you pass one. It sets up Elasticsearch when
`ESDIAG_OUTPUT_URL` is declared and Kibana when `ESDIAG_KIBANA_URL` is
declared.

`setup context` loads the Elastic CLI configuration from `--config`, then
`ELASTIC_CLI_CONFIG_FILE`, then your home directory. It uses the named context,
or the current context, and sets up whichever of its `elasticsearch` and
`kibana` services are defined. Credentials, including resolver values, stay in
memory and are never written to `hosts.yml` or the keystore.

### Trial license

Kibana's Agent Builder assets need an Enterprise or trial license. Setup never
changes a cluster's license on its own. When `setup output`, `setup env`, or
`setup context` installs both services and the license does not include Agent
Builder, it installs the Elasticsearch assets, skips Kibana, and reports a
`partial` outcome with a warning. To start a 30-day trial instead, pass
`--activate-trial` or set `ESDIAG_SETUP_ACTIVATE_TRIAL=true`:

```sh
esdiag setup output --activate-trial
```

A cluster can start only one trial. Serverless projects have no license to
change, so setup always continues to Kibana there. `setup elasticsearch` and
`setup kibana` never read or change the license.

### Agent skills

```sh
esdiag setup skills
esdiag setup skills claude codex
```

`setup skills` installs the embedded skill for detected coding agents, or for
the named `claude`, `codex`, and `opencode` agents. `--force` replaces locally
modified skill directories. Restart the agent after installation.

### Shell completions

```sh
esdiag setup completions zsh
esdiag setup completions zsh --print
```

By default, `setup completions` writes a conventional file under your home
directory and emits one `completion_installed` result with `shell`, `path`, and
`instruction`. It does not edit shell startup files. `--print` writes the raw
script to stdout instead and emits no YAML or JSON result.

| Shell | Install path | Follow-up |
|---|---|---|
| bash | `$XDG_DATA_HOME/bash-completion/completions/esdiag` when `XDG_DATA_HOME` is an absolute path; otherwise `~/.local/share/bash-completion/completions/esdiag` | Needs bash-completion enabled, or source the file from `~/.bashrc` |
| zsh | `~/.zfunc/_esdiag` | Add the printed `fpath` / `compinit` line to `~/.zshrc` if missing |
| fish | `$XDG_CONFIG_HOME/fish/completions/esdiag.fish` when `XDG_CONFIG_HOME` is an absolute path; otherwise `~/.config/fish/completions/esdiag.fish` | Open a new fish shell |
| powershell | `~/.config/powershell/Completions/esdiag.ps1` | Dot-source from your PowerShell profile; PowerShell does not autoload this path |

Completions cover subcommands and flags only. Saved host and job names are not
completed dynamically.

## `collect`

```sh
esdiag collect <HOST> <OUTPUT_DIR>
```

`<HOST>` must be a saved host with the `collect` role or an Elastic CLI target
reference. `<OUTPUT_DIR>` must exist. Collection creates an archive under that
directory.

```sh
elastic diag collect .es ~/diag-output
```

Common options:

- `--type minimal|light|standard|support`
- `--include` and `--exclude` for API selection
- `--sources <PATH>` for a product-matching sources file
- `--account`, `--case`, `--opportunity`, and `--user` for report metadata
- `--send <ID_OR_URL>` to send the collected archive to Elastic Upload Service
- `--save-job <NAME>` to save the command before it runs

`--send` does not delete the local archive.

## `process`

```sh
esdiag process <INPUT> [OUTPUT]
```

`<INPUT>` can be a support-diagnostics ZIP, an unpacked directory, a saved
host, an Elastic CLI target reference, or an Elastic Upload Service URL.

```sh
elastic diag process .prod.es .diag.es
```

`--ask <PROMPT>` processes the diagnostic, then starts an Agent Builder
conversation with the diagnostic ID in context. It needs a Kibana-enabled
output and cannot be used with output `-`.

`--save-job <NAME>` needs a saved `collect` host as input and an explicit
output. `--sources <PATH>` needs a source definition for the detected product.

## `serve`

```sh
esdiag serve [OUTPUT]
```

`--bind` selects the IPv4 address. It defaults to `ESDIAG_BIND`, then
`127.0.0.1`, so a native server only accepts local connections. Use
`--bind 0.0.0.0` to accept connections from other machines. The container
image sets `ESDIAG_BIND=0.0.0.0` so published ports reach the server.
`--port` defaults to `2501`. `--kibana` sets the Kibana link shown in the web
UI.

`--mode user` is for a local, single-user web server. `--mode service` is for
an administrator-run shared service. Service users open its URL. They do not
run `serve`, `init`, or `setup`.

Service mode uses one startup-defined exporter. It does not persist user hosts,
jobs, or keystore state. Use `--auth-provider google-iap|none` to select
request authentication. Use `none` only for controlled local testing.

The `google-iap` provider reads the user identity from
`X-Goog-Authenticated-User-Email` by default. Set `--identity-header <HEADER>`
or `ESDIAG_IDENTITY_HEADER` when the identity-aware proxy uses another header.
The command-line value takes precedence over the environment. Configure the
proxy to remove client-supplied values and set the trusted identity header.

For local service-mode development without an identity header, use:

```sh
esdiag serve --mode service --auth-provider none
```

Service mode does not show API key inputs or mount API key submission routes.
Run `esdiag serve --mode user` locally for direct API-key collection. Prefer a
saved host with its credential in the encrypted keystore.

In user mode, an incomplete local workflow opens the `/welcome` onboarding
flow. It creates or unlocks the encrypted keystore with a masked form and
configures the same persistent workflow as `esdiag init`. Service mode shows an
administrator-owned configuration notice instead and never writes local
onboarding state.

`ESDIAG_SERVICE_JOB_CAP` sets the global job cap.
`ESDIAG_SERVICE_OWNER_JOB_CAP` sets the per-user cap.

See [Use a shared ESDiag service](setup/shared-service.md).

## `send`

```sh
esdiag send <FILE_NAME> <UPLOAD_ID>
```

`<UPLOAD_ID>` can be an Elastic Upload Service ID or URL. `--api-url` changes
the service base URL.

## `agent`

```sh
esdiag agent ask "Analyze diagnostic <diagnostic-id>: <question>"
```

`agent ask` sends one prompt to the Agent Builder agent linked to the configured
output. It returns a message, conversation ID, and Kibana URL. To install the
coding-agent skill, use [`esdiag setup skills`](#agent-skills).

## `job`

```sh
esdiag job run <NAME>
esdiag job list
esdiag job delete <NAME>
```

Saved jobs refer to hosts and secrets by name. They do not store credentials.
