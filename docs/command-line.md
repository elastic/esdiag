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
| `setup` | Installs ESDiag assets in Elasticsearch and Kibana. |
| `agent` | Asks Kibana Agent Builder or installs the ESDiag Skill. |
| `job` | Runs, lists, and deletes saved jobs. |

## Global behavior

`--debug` writes debug logs. `--format yaml|json` selects the result format for
finite commands. YAML is the default.

Finite commands write one result to stdout. Progress and errors go to stderr.
When a command fails after it starts, stdout contains a `command_failed` result
and the command exits non-zero.

`process ... -` writes only NDJSON documents to stdout. It never appends a YAML
or JSON result. `serve` writes one readiness result after it binds unless its
exporter owns stdout.

## Local state and environment

By default, ESDiag stores user state in `~/.esdiag`:

| File | Contents |
|---|---|
| `hosts.yml` | Saved endpoints and secret references. |
| `secrets.yml` | Encrypted keystore. |
| `esdiag.yml` | Non-secret user defaults. |
| `settings.yml` | Saved UI settings. |
| `jobs.yml` | Saved jobs. |
| `last_run/` | Debug files from recent commands. |

`ESDIAG_HOSTS` and `ESDIAG_KEYSTORE` override the paths to `hosts.yml` and
`secrets.yml`.

These environment variables define an output when a command does not receive
one:

- `ESDIAG_OUTPUT_URL`
- `ESDIAG_OUTPUT_APIKEY`
- `ESDIAG_OUTPUT_USERNAME`
- `ESDIAG_OUTPUT_PASSWORD`
- `ESDIAG_KIBANA_URL`

ESDiag uses one complete output definition. It does not combine endpoints or
credentials from command arguments, environment variables, and `esdiag.yml`.

`LOG_LEVEL` sets the default log level. `ESDIAG_KEYSTORE_PASSWORD` supplies a
keystore password for non-interactive use.

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

Save HTTP URLs as hosts before using them as outputs. A raw `http://` or
`https://` argument is a file path, not an Elasticsearch destination.

## `host`

`host add`, `update`, `remove`, `list`, and `auth` manage entries in
`hosts.yml`.

Create a saved Elasticsearch host:

```sh
esdiag host add prod-es https://es.example.com:9200 \
  --app elasticsearch --roles send --secret prod-es
```

`host add` and `host update` test the full definition before saving it.
`host auth <NAME>` tests an existing definition without changing it.

Roles:

- `collect` is valid for Elasticsearch, Kibana, and Logstash.
- `send` is valid for Elasticsearch.
- `view` is valid for Kibana.

If you omit roles, the host gets `collect`.

Use `--secret <SECRET_ID>` to use a keystore entry. `--apikey` or
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

When you select local processing and no stack exists, `init` can start a
binary-owned core stack. Its approval includes that new stack's required
assets; declining returns to remote output setup.

See [Configure ESDiag](setup/configuration.md) for the prompts and paths.

## `local`

`esdiag local <command>` runs the binary's Rust-owned local-stack lifecycle:

```sh
esdiag local up
esdiag local status
esdiag local auth
esdiag local logs
esdiag local down
```

`up` accepts `--stack=auto|core|full`.

- `auto` defaults to core.
- `core` runs Elasticsearch and Kibana in containers and starts native
  `esdiag serve --mode user`.
- `full` runs the ESDiag web service in a container.

The state directory records the chosen mode. Changing modes does not move
hosts, jobs, settings, or secrets between the native user directory and the
full-mode container volume.

`esdiag local update` cannot update binary-owned lifecycle code. Update the
binary through Homebrew, Cargo, or its release archive.

See [Run a local diagnostic cluster](setup/esdiag-local.md) for local setup and
[local-stack launcher reference](bin/esdiag-local.md) for every launcher
option.

## `setup`

`esdiag setup [HOST]` installs or updates ESDiag templates and ingest pipelines.
With a saved host, it uses that host. Without one, it uses `ESDIAG_OUTPUT_*`.
Kibana setup also needs `ESDIAG_KIBANA_URL`.

```sh
esdiag setup diagnostics-output
```

Run setup before the first ingest and after an upgrade that changes assets.
The credential needs permission to install them.

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

`--bind` selects the IPv4 address. `--port` defaults to `2501`. `--kibana`
sets the Kibana link shown in the web UI.

`--mode user` is for a local, single-user web server. `--mode service` is for
an administrator-run shared service. Service users open its URL. They do not
run `serve`, `init`, or `setup`.

Service mode uses one startup-defined exporter. It does not persist user hosts,
jobs, or keystore state. Use `--auth-provider google-iap|none` to select
request authentication. Use `none` only for controlled local testing.

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
esdiag agent skills
```

`agent ask` sends one prompt to the Agent Builder agent linked to the configured
output. It returns a message, conversation ID, and Kibana URL.

`agent skills` installs the embedded skill for supported coding agents.
Use `--target claude`, `--target codex`, or `--target opencode` when needed.
Restart the agent after installation.

## `job`

```sh
esdiag job run <NAME>
esdiag job list
esdiag job delete <NAME>
```

Saved jobs refer to hosts and secrets by name. They do not store credentials.
