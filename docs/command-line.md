# Command-Line Interface Reference

This document is the primary command-line reference for `esdiag`.
It consolidates the older CLI notes from `readme.md`, fills in newer commands and options, and reconciles the docs with the current built binary and command behavior.

For the exact option text of your local build, prefer:

```sh
esdiag --help
esdiag <command> --help
```

## Overview

`esdiag` supports these main workflows:

- Configure and validate saved hosts with `esdiag host`
- Manage encrypted credentials with `esdiag keystore`
- Install Elasticsearch and Kibana assets with `esdiag setup`
- Collect fresh API diagnostics from a saved host with `esdiag collect`
- Process diagnostic input into Elasticsearch documents with `esdiag process`
- Run a local upload/UI service with `esdiag serve`
- Upload raw archives to Elastic Upload Service with `esdiag upload`

Current top-level help:

```text
Elastic Stack Diagnostics (esdiag) - collect diagnostics and import into Elasticsearch

Usage: esdiag [OPTIONS] [COMMAND]

Commands:
  collect   Collect a diagnostic bundle from a known host's API endpoints, writes output to a directory
  serve     Start a web server to receive diagnostic bundle uploads
  host      Configure, test and save a remote host connection to `~/.esdiag/hosts.yml`
  keystore  Manage encrypted secrets in the local keystore
  process   Receives a diagnostic from the input, processes it, and sends processed docs to the output
  upload    Upload a raw diagnostic archive to Elastic Upload Service
  setup     Import assets (templates, ingest pipelines, etc.) to a known Elasticsearch host
  help      Print this message or the help of the given subcommand(s)

Options:
      --debug    Enable debug logging
  -h, --help     Print help
  -V, --version  Print version
```

## Global Behavior

### Global options

- `--debug` enables debug logging for any command.
- `--help` and `--version` work at the top level.
- Logging defaults to `info` unless overridden by `--debug` or `LOG_LEVEL`.

### No-command behavior

- If `esdiag` is run with CLI arguments but no subcommand, it exits with a usage error.
- If `esdiag` is run with no arguments at all, desktop startup may occur in builds that include the desktop feature.
- In non-desktop builds, running with no command prints help and exits with an error.

## Configuration Files And Environment

### Default local files

By default, `esdiag` stores local state under `~/.esdiag/`:

- `hosts.yml`: saved host definitions
- `secrets.yml`: encrypted keystore backing `--secret` references
- `settings.yml`: saved UI/runtime settings such as active target selection
- `last_run/`: debug artifacts from processing and related commands

### Path overrides

These environment variables change where local state is read and written:

- `ESDIAG_HOSTS`: override the path to `hosts.yml`
- `ESDIAG_KEYSTORE`: override the path to `secrets.yml`

### Common runtime environment variables

- `LOG_LEVEL`: default logging level when `--debug` is not used
- `ESDIAG_KEYSTORE_PASSWORD`: non-interactive password source for keystore-backed operations
- `ESDIAG_OUTPUT_URL`: default Elasticsearch output URL when output is omitted
- `ESDIAG_OUTPUT_APIKEY`: default output API key
- `ESDIAG_OUTPUT_USERNAME`: default output username
- `ESDIAG_OUTPUT_PASSWORD`: default output password
- `ESDIAG_KIBANA_URL`: Kibana URL used by `serve`, processing metadata, and host-omitted setup flows
- `ESDIAG_KIBANA_SPACE`: optional Kibana space appended to generated Kibana links
- `ESDIAG_MODE`: runtime mode for `serve` when `--mode` is omitted; valid values are `user` and `service`
- `ESDIAG_OUTPUT_TASK_LIMIT`: task concurrency limit used by the Elasticsearch exporter

## Output Resolution Rules

Several commands accept an optional output target. The current resolution rules are:

- `-` means stdout
- A string matching a saved host name resolves to that known host
- Any other non-empty string is treated as a local filesystem target
- If output is omitted entirely, `esdiag` falls back to `ESDIAG_OUTPUT_URL` plus optional auth env vars

This applies to:

- `esdiag process [OUTPUT]`
- `esdiag serve [OUTPUT]`

Raw `http://` or `https://` strings are not treated as direct output targets unless they are saved and resolved as known hosts.

## `host`

Use `esdiag host` to create, validate, update, or delete saved host definitions in `hosts.yml`.

Current help:

```text
Configure, test and save a remote host connection to `~/.esdiag/hosts.yml`

Usage: esdiag host [OPTIONS] <NAME> [APP] [URL]

Arguments:
  <NAME>  A name to identify this host
  [APP]   Application of this host (elasticsearch, kibana, logstash, etc.)
  [URL]   A host URL to connect to

Options:
      --accept-invalid-certs <ACCEPT_INVALID_CERTS>
          Accept invalid certificates [possible values: true, false]
      --debug
          Enable debug logging
      --delete
          Delete the saved host configuration
  -a, --apikey <APIKEY>
          ApiKey, passed as http header
  -u, --user <USERNAME>
          Username for authentication [aliases: --username]
  -p, --password <PASSWORD>
          Password for authentication
      --secret <SECRET>
          Secret identifier in the encrypted keystore
      --roles <ROLES>
          Comma-separated host roles
  -n, --nosave
          Don't save the host configuration on successful connection
  -h, --help
          Print help
```

### Host modes

`esdiag host` has four practical modes:

1. Create or replace:
   Supply `NAME`, `APP`, and `URL` to build a host definition from scratch.
2. Validate only:
   Supply only `NAME` for an existing host to re-test the saved definition without changing it.
3. Incremental update:
   Supply only `NAME` plus mutable flags such as `--secret`, `--apikey`, `--roles`, or `--accept-invalid-certs`.
4. Delete:
   Supply `NAME --delete` to remove a saved host.

### Authentication options

- `--apikey <APIKEY>` stores explicit API key auth in the host
- `--user <USERNAME> --password <PASSWORD>` stores explicit basic auth
- `--secret <SECRET_ID>` stores only the keystore reference in the host and resolves credentials from `secrets.yml`

If a secret-backed host is updated with explicit `--apikey` or `--user`/`--password`, the saved secret reference is replaced by the new explicit auth source.

### Certificate validation

`--accept-invalid-certs` now takes an explicit boolean value:

- `--accept-invalid-certs true`: enable invalid-certificate acceptance
- `--accept-invalid-certs false`: disable it

For incremental updates:

- if the flag is omitted, the saved value is preserved
- if provided, the saved value is updated

### Roles

Use `--roles` with comma-separated values:

- `collect`
- `send`
- `view`

Role validation rules enforced by the saved host model:

- `collect` is valid for any host type
- `send` is valid only for Elasticsearch hosts
- `view` is valid only for Kibana hosts
- omitted roles default to `collect`

### Update behavior

For saved-host updates:

- omitted fields are preserved
- the merged saved host is always connection-tested before it is written back
- unknown host names fail unless `APP` and `URL` are supplied to create a new record

### Delete behavior

Use:

```sh
esdiag host <name> --delete
```

Delete notes:

- `--delete` is mutually exclusive with create/update flags
- deleting an unknown host returns an explicit error
- deleting a host also updates local saved settings when that host was the active saved target

### Examples

```sh
# Create a saved Elasticsearch host
esdiag host prod-es elasticsearch http://localhost:9200

# Create a host backed by a keystore secret
esdiag host prod-es elasticsearch http://localhost:9200 --secret prod-es-apikey

# Create a host with explicit workflow roles
esdiag host prod-es elasticsearch http://localhost:9200 --roles collect,send

# Re-validate an existing saved host without changing it
esdiag host prod-es

# Rotate a saved host to a new secret reference
esdiag host prod-es --secret prod-es-rotated

# Change the saved certificate policy in place
esdiag host prod-es --accept-invalid-certs false

# Delete a saved host
esdiag host prod-es --delete
```

## `keystore`

Use `esdiag keystore` to manage encrypted auth material stored separately from `hosts.yml`.

Current help:

```text
Manage encrypted secrets in the local keystore

Usage: esdiag keystore [OPTIONS] <COMMAND>

Commands:
  add      Add or update a secret in the encrypted keystore
  remove   Remove a secret from the encrypted keystore
  migrate  Migrate legacy host credentials in hosts.yml into the keystore
  help     Print this message or the help of the given subcommand(s)

Options:
      --debug  Enable debug logging
  -h, --help   Print help
```

### `keystore add`

```text
Add or update a secret in the encrypted keystore

Usage: esdiag keystore add [OPTIONS] <SECRET_ID>

Arguments:
  <SECRET_ID>  Secret identifier

Options:
      --debug                Enable debug logging
  -u, --user <USERNAME>      Username for authentication [aliases: --username]
  -p, --password <PASSWORD>  Password for authentication
  -a, --apikey <APIKEY>      ApiKey, passed as http header
  -h, --help                 Print help
```

Use either:

- `--apikey <value>`
- `--user <name> --password <value>`

### `keystore remove`

```text
Remove a secret from the encrypted keystore

Usage: esdiag keystore remove [OPTIONS] <SECRET_ID>

Arguments:
  <SECRET_ID>  Secret identifier

Options:
      --debug                Enable debug logging
  -u, --user <USERNAME>      Username for authentication [aliases: --username]
  -p, --password <PASSWORD>  Password for authentication
  -a, --apikey <APIKEY>      ApiKey, passed as http header
  -h, --help                 Print help
```

You can remove by secret ID alone, or provide auth flags when you want the removal to be scoped to an expected auth shape.

### `keystore migrate`

`esdiag keystore migrate` moves legacy plaintext credentials from saved hosts into the encrypted keystore and rewrites those hosts to reference the migrated secret by host name.

### Keystore password behavior

- `ESDIAG_KEYSTORE_PASSWORD` enables non-interactive use
- in an interactive shell, `keystore add` and `keystore remove` can prompt for the password if the env var is unset
- non-interactive secret-backed operations fail clearly when the password is unavailable

### Examples

```sh
# Add a basic auth secret
esdiag keystore add prod-es-basic --user elastic --password changeme

# Add an API key secret
esdiag keystore add prod-es-apikey --apikey BASE64_ENCODED_KEY

# Remove a secret by id
esdiag keystore remove prod-es-apikey

# Migrate legacy hosts.yml credentials into the keystore
esdiag keystore migrate
```

## `setup`

Use `esdiag setup` to install or refresh Elasticsearch-side assets.

Current help:

```text
Import assets (templates, ingest pipelines, etc.) to a known Elasticsearch host

Usage: esdiag setup [OPTIONS] [HOST]

Arguments:
  [HOST]  Known Elasticsearch host to import assets into; if omitted the ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, ESDIAG_OUTPUT_PASSWORD variables will be checked.

Options:
      --debug  Enable debug logging
  -h, --help   Print help
```

### Behavior

- With `[HOST]`, setup targets that saved Elasticsearch host
- Without `[HOST]`, setup uses:
  - `ESDIAG_OUTPUT_URL`
  - `ESDIAG_OUTPUT_APIKEY`
  - `ESDIAG_OUTPUT_USERNAME`
  - `ESDIAG_OUTPUT_PASSWORD`
- In host-omitted mode, Kibana asset setup also relies on `ESDIAG_KIBANA_URL`

Run setup before the first ingest into a cluster and again after version changes that may require refreshed templates, pipelines, or dashboards.

### Examples

```sh
# Setup a saved output cluster
esdiag setup prod-es

# Setup using environment-driven output
ESDIAG_OUTPUT_URL=http://localhost:9200 esdiag setup
```

## `process`

Use `esdiag process` to transform diagnostic input into Elasticsearch-friendly documents and send them to an output target.

Current help:

```text
Receives a diagnostic from the input, processes it, and sends processed docs to the output

Usage: esdiag process [OPTIONS] <INPUT> [OUTPUT]

Arguments:
  <INPUT>
          Source to read diagnostic data from (archive, directory, known host or Elastic uploader URL)

  [OUTPUT]
          Target to send the processed diagnostic documents to (known host, file, stdout, or env). Strings will be checked against the known hosts stored in `~/.esdiag/hosts.yml` and will fallback to a filename if not found. Use `-` for stdout. If nothing is provided, the output will try using the environment variables: ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, and ESDIAG_OUTPUT_PASSWORD.

Options:
  -a, --account <ACCOUNT>
          Diagnostic report account name

      --debug
          Enable debug logging

  -c, --case <CASE>
          Diagnostic report case number

  -o, --opportunity <OPPORTUNITY>
          Diagnostic report opportunity

  -u, --user <USER>
          Diagnostic report user

      --sources <SOURCES>
          Override the embedded sources.yml for the detected Elasticsearch or Logstash workflow. The file must match the active product or the command fails before processing

  -h, --help
          Print help (see a summary with '-h')
```

### Supported input forms

- support-diagnostics `.zip` archive
- unpacked diagnostic directory
- saved known host name
- Elastic Upload Service URL

### Output forms

- saved known host
- local file path
- `-` for stdout
- omitted output, using `ESDIAG_OUTPUT_*`

### Metadata options

These annotate the generated report context:

- `--account`
- `--case`
- `--opportunity`
- `--user`

### `--sources`

Use `--sources <path>` when endpoint definitions must come from a custom or version-specific `sources.yml`. The file must match the detected product when processing a host-backed Elasticsearch or Logstash workflow.

### Examples

```sh
# Process a local archive to a saved output host
esdiag process ~/Downloads/api-diagnostic.zip prod-es

# Process a directory to stdout
esdiag process ~/Downloads/api-diagnostic-dir -

# Process with an environment-driven output
ESDIAG_OUTPUT_URL=http://localhost:9200 esdiag process ~/Downloads/api-diagnostic.zip
```

## `collect`

Use `esdiag collect` when you need fresh API diagnostics from a saved host.

Current help:

```text
Collect a diagnostic bundle from a known host's API endpoints, writes output to a directory

Usage: esdiag collect [OPTIONS] <HOST> <OUTPUT>

Arguments:
  <HOST>    The Elastic Stack host to collect diagnostics from
  <OUTPUT>  An existing directory to create a diagnostic directory and files in

Options:
      --debug                      Enable debug logging
      --type <TYPE>                Diagnostic type (minimal, light, standard, support) [default: standard]
      --include <INCLUDE>          Comma-separated list of APIs to include
      --exclude <EXCLUDE>          Comma-separated list of APIs to exclude
      --sources <SOURCES>          Override the embedded sources.yml for the detected Elasticsearch or Logstash workflow. The file must match the active product or the command fails before collection
  -a, --account <ACCOUNT>          Diagnostic report account name
  -c, --case <CASE>                Diagnostic report case number
  -o, --opportunity <OPPORTUNITY>  Diagnostic report opportunity
  -u, --user <USER>                Diagnostic report user
      --upload <UPLOAD_ID>         Elastic Upload Service upload id or URL for immediate upload after collection
  -h, --help                       Print help
```

### Behavior

- `<HOST>` must resolve to a saved known host
- the host must carry the `collect` role
- `<OUTPUT>` must already exist
- `esdiag` creates a diagnostic directory or archive structure within that output directory
- `--upload` uploads the exact collected archive after a successful collect run; the archive still remains on disk locally

### Collection level

`--type` accepts:

- `minimal`
- `light`
- `standard`
- `support`

### API selection

- `--include` narrows to a comma-separated list of APIs
- `--exclude` removes a comma-separated list of APIs
- `--sources` overrides the embedded endpoint definitions for Elasticsearch or Logstash workflows

### Metadata options

- `--account`
- `--case`
- `--opportunity`
- `--user`

### Upload handoff

- `--upload` accepts an Elastic Upload Service upload id or URL
- upload starts only after collection succeeds
- upload uses the runtime-resolved archive path, so you do not need to know the generated filename ahead of time

### Examples

```sh
# Collect a standard diagnostic
esdiag collect prod-es ~/diag-output

# Collect a minimal diagnostic with an explicit API subset
esdiag collect prod-es ~/diag-output --type minimal --include nodes,cluster_health

# Collect and immediately upload the resulting archive
esdiag collect prod-es ~/diag-output --upload abc123
```

## `serve`

Use `esdiag serve` to run the local web/API service for uploads and processing.

Current help:

```text
Start a web server to receive diagnostic bundle uploads

Usage: esdiag serve [OPTIONS] [OUTPUT]

Arguments:
  [OUTPUT]
          Target to send the processed diagnostic documents to (known host, file, stdout, or env). Strings will be checked against the known hosts stored in `~/.esdiag/hosts.yml` and will fallback to a filename if not found. Use `-` for stdout. If nothing is provided, the output will try using the environment variables: ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, and ESDIAG_OUTPUT_PASSWORD.

Options:
      --debug
          Enable debug logging

  -p, --port <PORT>
          The port to bind the server to

          [default: 2501]

      --mode <MODE>
          Web runtime mode: user or service

          [possible values: service, user]

      --kibana <KIBANA>
          Kibana URL to display in the web interface. If not provided, will use the ESDIAG_KIBANA_URL environment variable.

  -h, --help
          Print help (see a summary with '-h')
```

### Runtime mode

`--mode` accepts:

- `user`
- `service`

If omitted, `serve` checks `ESDIAG_MODE`. Invalid values fail clearly.

### Kibana URL behavior

- `--kibana <URL>` overrides the displayed Kibana base URL
- if omitted, `ESDIAG_KIBANA_URL` is used
- if `ESDIAG_KIBANA_SPACE` is set, the space path is appended in generated links

### Output handling

`serve` uses the same output target rules as `process`.

### Examples

```sh
# Start the service on the default port with a saved output host
esdiag serve prod-es

# Start the service on port 8080
esdiag serve --port 8080 prod-es

# Start in service mode with env-driven output
ESDIAG_MODE=service ESDIAG_OUTPUT_URL=http://localhost:9200 esdiag serve
```

## `upload`

Use `esdiag upload` to send a raw archive to Elastic Upload Service.

Current help:

```text
Upload a raw diagnostic archive to Elastic Upload Service

Usage: esdiag upload [OPTIONS] <FILE_NAME> <UPLOAD_ID>

Arguments:
  <FILE_NAME>  Local diagnostic archive file path
  <UPLOAD_ID>  Upload id or Elastic Upload Service URL

Options:
      --api-url <API_URL>  Elastic Upload Service base URL [default: https://upload.elastic.co]
      --debug              Enable debug logging
  -h, --help               Print help
```

### Behavior

- `<FILE_NAME>` is the local archive to upload
- `<UPLOAD_ID>` can be either:
  - a raw upload id
  - a full Elastic Upload Service URL
- `--api-url` overrides the upload service base URL

### Example

```sh
esdiag upload diag.zip abc123
```

## Command Discovery

Use these forms when you need authoritative syntax for your build:

```sh
esdiag help
esdiag help host
esdiag host --help
esdiag serve --help
```

## Troubleshooting

- If a saved-host update fails, remember that `esdiag host <NAME>` re-validates the merged host live before saving it.
- If a saved host should be removed entirely, use `esdiag host <NAME> --delete`.
- If output lands in the wrong place, verify whether your output argument matched a saved host name before being treated as a file path.
- If keystore-backed auth fails, check `ESDIAG_KEYSTORE_PASSWORD` and confirm the referenced secret id exists.
- If setup or ingestion behavior changes after a version upgrade, re-run `esdiag setup`.
- If no-command startup behaves unexpectedly, check whether your build includes desktop support.
