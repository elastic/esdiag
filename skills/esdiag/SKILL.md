---
name: esdiag
description: Operate Elastic Stack Diagnostics (`esdiag`) end-to-end for host configuration, asset setup, diagnostic collection, processing, and upload-service workflows. Use when a user asks to run or troubleshoot `esdiag` commands (`host`, `setup`, `collect`, `process`, `serve`), wire output targets via known hosts or `ESDIAG_OUTPUT_*` environment variables, process support-diagnostics archives/directories/upload links, or expose the local web/API service.
---

# Esdiag

Use this skill to choose and run the right `esdiag` command sequence quickly and safely.

Prefer command output from `esdiag help` and `esdiag <command> --help` over memory when behavior is unclear.

## Workflow Decision Tree

- If the task is "save/test a connection", run `esdiag host`.
- If the task is "install templates/pipelines/assets", run `esdiag setup`.
- If the task is "transform diagnostics into documents and send somewhere", run `esdiag process`.
- If the task is "collect API diagnostics from a host into local files", run `esdiag collect`.
- If the task is "accept browser uploads or API submissions", run `esdiag serve`.

## Standard Flow

1. Configure output host or output environment variables.
2. Run `esdiag setup` against the Elasticsearch output target.
3. Ingest diagnostics via `esdiag process` or `esdiag serve`.
4. Confirm completion output and share destination details (host/file/stdout).

## Step 1: Configure Hosts and Auth

- Use `esdiag host [OPTIONS] <NAME> [APP] [URL]` to test and save host configuration to `~/.esdiag/hosts.yml`.
- Use `--apikey` for API key auth or `--username`/`--password` for basic auth.
- Use `--accept-invalid-certs` for lab/self-signed environments.
- Use `--nosave` for connectivity tests that should not persist.
- Use `.env`/environment variables when the user does not want a saved host.

## Step 2: Setup Output Cluster

- Run `esdiag setup [HOST]` before first ingestion into a cluster.
- If `[HOST]` is omitted, rely on:
  - `ESDIAG_OUTPUT_URL`
  - `ESDIAG_OUTPUT_APIKEY`
  - `ESDIAG_OUTPUT_USERNAME`
  - `ESDIAG_OUTPUT_PASSWORD`

## Step 3: Process Diagnostics

- Use `esdiag process [OPTIONS] <INPUT> [OUTPUT]`.
- Accept these input patterns:
  - Support diagnostics `.zip` archive
  - Unpacked diagnostic directory
  - Known host name from `~/.esdiag/hosts.yml`
  - Elastic Upload URL (`https://token:...@upload.elastic.co/d/...`)
- Resolve `[OUTPUT]` in this order:
  - Known host name
  - Filename fallback
  - `-` for stdout
  - Environment-variable output target if omitted
- Attach report metadata when provided by user:
  - `--account`
  - `--case`
  - `--opportunity`
  - `--user`

## Step 4: Collect Then Process (Optional)

- Use `esdiag collect <HOST> <OUTPUT>` when the user needs fresh API diagnostics.
- Ensure `<OUTPUT>` already exists; command creates a diagnostic subdirectory within it.
- For repeated captures, use `bin/min-diag.sh watch` and process each generated directory with `esdiag process`.

## Step 5: Run Upload Service (Optional)

- Use `esdiag serve [OPTIONS] [OUTPUT]` to host upload and API endpoints.
- Default port is `2501`; override with `--port`.
- Pass `--kibana <URL>` (or set `ESDIAG_KIBANA_URL`) to show direct links in UI flows.
- Use output resolution rules from `process`.

## Troubleshooting Rules

- If command behavior looks inconsistent with docs, trust live help output first.
- If auth fails, re-check saved host/app/url/auth mode and whether cert validation is required.
- If output is not where expected, verify `[OUTPUT]` parsing and known-host name collisions with filenames.
- If setup or ingest fails after version changes, rerun `esdiag setup` before retrying `process`.

## References

- Use `references/cli.md` for command syntax and option details.
