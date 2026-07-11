---
name: esdiag
description: Collect or process Elasticsearch, Kibana, and Logstash diagnostics with `esdiag`. Use for collecting live API diagnostics from a cluster, processing support bundle archives or Elastic upload links, sending results to an output cluster, managing saved hosts and encrypted credentials, running saved diagnostic jobs, or hosting the web user interface.
---

# ESDiag

Use this skill to choose and run the right `esdiag` command sequence safely.

Prefer live help output over memory when behavior is unclear:

```sh
esdiag --help
esdiag <command> --help
```

## Command Routing

- Connection management: `esdiag host`
- Credentials and unlock state: `esdiag keystore`
- Asset setup: `esdiag setup`
- Process diagnostics into output docs: `esdiag process`
- Collect fresh API diagnostics: `esdiag collect`
- Saved reusable jobs: `esdiag job`, or `--save-job <NAME>` on compatible `collect`/`process`
- Web/API intake: `esdiag serve`

## Required Checks

Run `esdiag keystore status` before authenticated collection, processing from saved hosts, saved jobs, or host/keystore changes.

If locked, stop and ask the user to unlock with `esdiag keystore unlock` or through the web UI.

```
esdiag keystore status
```

- `Keystore: unlocked until <date> (duration)` — proceed normally.
- `Keystore: locked` — stop and tell the user to unlock it via the web UI or with `esdiag keystore unlock` before continuing.

## Detailed Workflow

- Use `esdiag host add <NAME> <APP> <URL>` to create and save a new host definition in `~/.esdiag/hosts.yml`.
- Use `esdiag host update <NAME>` with mutable flags to modify an existing saved host in place.
- Use `esdiag host remove <NAME>` to delete an existing saved host from `hosts.yml`.
- Use `esdiag host list` to print a compact inventory of saved hosts (`name`, `app`, `secret`).
- Use `esdiag host auth <NAME>` to test a saved host's persisted authentication and connection settings without modifying it.
- Use `--apikey` for API key auth or `--user`/`--password` for basic auth.
- `--user` is the primary basic-auth flag (with `--username` available as an alias).
- Use `--secret <secret_id>` to reference credentials stored in the encrypted keystore.
- Use `--secret`, `--apikey`, `--user`/`--password`, and `--roles` with `host add` or `host update` to set authentication and workflow roles.
- Use `--roles collect,send,view` to assign host workflow roles.
- Use `--accept-invalid-certs true` to enable invalid-certificate acceptance for a saved host, and `--accept-invalid-certs false` to remove it. If the flag is omitted during `host update`, the saved certificate setting is preserved.
- `host add` fails if the host already exists; `host update`, `host remove`, and `host auth` fail if the host does not exist.
- `host update` always re-tests the live connection before persistence.
- Use environment variables (optionally by sourcing a `.env` file in the shell) when the user does not want a saved host.

## Manage Encrypted Secrets

- Use `esdiag keystore add <secret_id>` to create encrypted credentials.
- Use `esdiag keystore update <secret_id>` to change an existing encrypted secret.
  - Basic auth: `--user <name> --password <value>` or omit the password value in an interactive shell to get a masked prompt.
  - API key auth: `--apikey <value>` or omit the value in an interactive shell to get a masked prompt.
- Use `esdiag keystore remove <secret_id>` to remove encrypted credentials (optionally scoped by auth type flags).
- Use `esdiag keystore unlock [--ttl 24h|7d|90m]` to cache keystore access for later CLI runs, `status` to inspect it, and `lock` to clear it.
- Use `esdiag keystore password` to rotate the keystore password.
- Use `esdiag keystore migrate` to move legacy plaintext host credentials from `hosts.yml` into keystore entries keyed by host name.
- Set `ESDIAG_KEYSTORE_PASSWORD` for non-interactive keystore operations.
- In interactive shells, `keystore add/update/remove/unlock/password` can prompt for the keystore password when `ESDIAG_KEYSTORE_PASSWORD` is unset.

## Setup Output Cluster

- Run `esdiag setup [HOST]` before first ingestion into a cluster.
- If `[HOST]` is omitted, rely on:
  - `ESDIAG_OUTPUT_URL`
  - `ESDIAG_OUTPUT_APIKEY`
  - `ESDIAG_OUTPUT_USERNAME`
  - `ESDIAG_OUTPUT_PASSWORD`
  - `ESDIAG_KIBANA_URL` (required for Kibana asset setup in host-omitted mode)
- In host-omitted mode, `setup` attempts both Elasticsearch and Kibana asset setup.

## Process Diagnostics

- Use `esdiag process [OPTIONS] <INPUT> [OUTPUT]`.
- Accept these input patterns:
  - Support diagnostics `.zip` archive
  - Unpacked diagnostic directory
  - Known host name from `~/.esdiag/hosts.yml`
  - Elastic Upload URL (`https://token:...@upload.elastic.co/d/...`)
- Resolve `[OUTPUT]` using these rules:
  - If `[OUTPUT]` is `-`, write to stdout.
  - Otherwise, if it matches a saved host name, use that host.
  - Otherwise, treat it as a filesystem target (file or directory).
  - If `[OUTPUT]` is omitted entirely, fall back to `ESDIAG_OUTPUT_*` environment variables (Elasticsearch output target).
  - Do not treat raw `http(s)` output strings as valid output targets unless they are saved and resolved as known hosts.
- Attach report metadata when provided by user:
  - `--account`
  - `--case`
  - `--opportunity`
  - `--user`
- Use `--sources` to override endpoint source definitions when testing new API mappings or reproducing source-selection behavior.
- After a successful `esdiag process`, if the output contains a `Kibana Link: <url>` line, present that URL to the user as a clickable markdown link. Do not manually look up Kibana hosts.
- Use `--save-job <NAME>` on compatible `process` invocations to persist the job before execution. The input must be a saved known host, and `[OUTPUT]` must be explicit.

## Collect Diagnostics

- Use `esdiag collect [OPTIONS] <HOST> <OUTPUT>` when the user needs fresh API diagnostics.
- Ensure `<OUTPUT>` already exists; command creates a diagnostic subdirectory within it.
- Use `--type` to control collection level, in ascending breadth: `minimal` (cluster + nodes only) → `light` (light-tagged APIs) → `standard` (fixed ~20 API set) → `support` (every available API in the sources definition).
- Use `--include` and `--exclude` to explicitly control which APIs are collected.
- Use metadata options (`--account`, `--case`, `--opportunity`, `--user`) when collected artifacts should carry report context.
- Use `--sources` when the collection endpoints should come from a non-default `sources.yml`.
- Use `--save-job <NAME>` on compatible `collect` invocations to persist the job before execution. `<HOST>` must be a saved host with the `collect` role, and `<OUTPUT>` must be an existing directory.
- For repeated captures, use `bin/min-diag.sh watch` and process each generated directory with `esdiag process`.

## Saved Jobs

Saved jobs persist named diagnostic configurations to `~/.esdiag/jobs.yml` so they can be re-run without reconfiguration. Jobs are saved from the `/jobs` web UI or managed via the CLI. Requires the `keystore` feature (enabled by default).

- Use `esdiag job list` to print all saved jobs as a table (name, collection target, processing level, send target).
  - Stale host references (hosts no longer in `hosts.yml`) are highlighted in red.
  - Prints nothing when no jobs exist.
- Use `esdiag job run <NAME>` to execute a saved job end-to-end (collect → process → send).
  - Resolves the collection host from `hosts.yml` and credentials from the keystore.
  - Fails with a clear error if the job name is unknown, the jobs file is missing, or the referenced host no longer exists.
- Use `esdiag job delete <NAME>` to remove a saved job from `jobs.yml`.
  - Fails with a clear error if the job name is not found.
- In the web UI (`/jobs` page, user mode only), the left panel lists saved jobs with Load and Delete actions. The Save form derives a default name from the workflow (`{host}-{action}-{destination}`) and disables saving for upload-file and service-link sources since those are not repeatable.

## Run Upload Service

- Use `esdiag serve [OPTIONS] [OUTPUT]` to host upload and API endpoints.
- Default port is `2501`; override with `--port`.
- Pass `--kibana <URL>` (or set `ESDIAG_KIBANA_URL`) to show direct links in UI flows.
- Use output resolution rules from `process`.

## Workflow Notes

- Configure an output host or `ESDIAG_OUTPUT_*` before processing into Elasticsearch.
- Run `esdiag setup [HOST]` before first ingestion into a cluster.
- Use `--sources <path/to/sources.yml>` for custom or version-specific API endpoint definitions.
- For `process`, resolve output as stdout (`-`), saved host, filesystem target, or `ESDIAG_OUTPUT_*` when omitted. Do not use raw HTTP URLs as output targets unless saved as hosts.
- For `collect`, `<HOST>` must be a saved host with the `collect` role and `<OUTPUT>` must be an existing directory.
- Use metadata flags (`--account`, `--case`, `--opportunity`, `--user`) when reports need context.
- If `process` prints a `Kibana Link: <url>`, present it as a clickable markdown link.

## Saved Jobs

- Use `--save-job <NAME>` on compatible `collect` or `process` invocations to persist the job before execution. Requires a saved known-host collection input; `process` also requires an explicit output. See `references/cli.md` for command-specific details.
- Use `esdiag job list`, `esdiag job run <NAME>`, and `esdiag job delete <NAME>` to manage saved jobs.
- Saved jobs require persisted known hosts and the keystore feature.

## Troubleshooting Rules
- If command behavior looks inconsistent with docs, trust live help output first.
- If auth fails, re-check saved host/app/url/auth mode and whether cert validation is required.
- If a saved-host update fails, remember that `esdiag host update <NAME>` re-validates the merged host definition live before saving it.
- If a host should be removed entirely, prefer `esdiag host remove <NAME>` instead of hand-editing `hosts.yml`.
- If output is not where expected, verify `[OUTPUT]` parsing and known-host name collisions with filenames.
- If setup or ingest fails after version changes, rerun `esdiag setup` before retrying `process`.

## References

- Use `references/cli.md` for command syntax, option details, and output resolution rules.
- Use `references/env-vars.md` for all `ESDIAG_*` environment variables and their purpose.
