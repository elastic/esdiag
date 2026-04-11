# ESDiag CLI Behavior Notes

Use this file for command behavior that is easy to misremember. For complete and version-accurate syntax, run `esdiag --help` and `esdiag <command> --help`.

## Top-level

```text
Usage: esdiag [OPTIONS] <COMMAND>

Commands:
  collect
  serve
  host
  process
  setup
  help

Options:
      --debug
      --sources <SOURCES>
  -h, --help
  -V, --version
```

## collect

```text
Usage: esdiag collect [OPTIONS] <HOST> <OUTPUT>
```

- `<HOST>`: known Elasticsearch host to collect from.
- `<OUTPUT>`: existing directory where a diagnostic directory is created.
- Common options:
  - `--type <TYPE>` (`minimal`, `light`, `standard`, `support`; default `standard`)
  - `--include <INCLUDE>` (comma-separated APIs)
  - `--exclude <EXCLUDE>` (comma-separated APIs)
  - `-a, --account <ACCOUNT>`
  - `-c, --case <CASE>`
  - `-o, --opportunity <OPPORTUNITY>`
  - `-u, --user <USER>`
- Use `esdiag collect --help` for the full version-specific option list.

## host

```text
Usage: esdiag host <COMMAND>
```

Commands:
- `add <NAME> <TARGET> [--app <APP>]`
- `update <NAME>`
- `remove <NAME>`
- `list`
- `auth <TARGET>`

Shared host auth/update options:
- `--accept-invalid-certs <true|false>`
- `-k, --apikey <APIKEY>`
- `-u, --username <USERNAME>`
- `-p, --password <PASSWORD>`
- `--secret <SECRET>`
- `--roles <ROLES>`
- `--app <APP>` when `<TARGET>` is ambiguous
- `--url-template` to persist `<TARGET>` as a reusable template

Host target modes:
- Concrete URL: `esdiag host add prod-es http://localhost:9200 --app elasticsearch`
- Template definition: `esdiag host add elastic-cloud "https://cloud.elastic.co/api/v1/deployments/{id}/{product}/main-{product}/proxy" --url-template`
- Materialize from template: `esdiag host add prod-es elastic-cloud://1234/elasticsearch`

Template reference examples:
- Elastic Cloud Hosted: `elastic-cloud://1234/kibana`
- ECE-style template:
  `esdiag keystore add ece_admin --apikey`
  `esdiag host add ece "https://coord.example.com:12443/api/v1/clusters/elasticsearch/{id}/{product}/_proxy" --url-template --secret ece_admin`

Notes:
- Supported placeholders are `{id}` and `{product}`.
- Bare template auth gives guidance: `esdiag host auth elastic-cloud`
- Omitted `product` defaults to `elasticsearch`: `elastic-cloud://1234`

## process

```text
Usage: esdiag process [OPTIONS] <INPUT> [OUTPUT]
```

Input resolution (in order):
1. `.zip` archive path
2. Unpacked diagnostic directory path
3. Known host name from `~/.esdiag/hosts.yml`
4. Elastic Upload URL (`https://token:...@upload.elastic.co/d/...`)

Output resolution (in order):
1. `-` → write to stdout
2. Matches a saved host name → send to that host
3. Any other string → filesystem target (file or directory)
4. Omitted → fall back to `ESDIAG_OUTPUT_*` env vars
- Do not pass raw `http(s)` URLs as output; save them as hosts first.

`--save-job <NAME>` persists a compatible known-host process invocation before execution. The input must be a saved host with the `collect` role, and `[OUTPUT]` must be explicit so the saved job has deterministic output.

## `collect`

- `<HOST>` must be a saved host with the `collect` role.
- `<OUTPUT>` must be an existing local directory. The collector creates the diagnostic archive under that directory.
- `--save-job <NAME>` persists the compatible known-host collect invocation before execution.
- Without `--upload`, a saved collect job stores `<OUTPUT>` as its final `output_dir`.
- With `--upload <UPLOAD_ID>`, `--save-job` saves a collect-and-upload job instead of a collect-to-directory job.

Common collection/report flags:
- `--type <TYPE>` (`minimal`, `light`, `standard`, `support`; default `standard`)
- `--include <INCLUDE>`
- `--exclude <EXCLUDE>`
- `--account <ACCOUNT>`
- `--case <CASE>`
- `--opportunity <OPPORTUNITY>`
- `--user <USER>`

## Saved Jobs

- Manage persisted jobs with `esdiag job list`, `esdiag job run <NAME>`, and `esdiag job delete <NAME>`.
- Saved jobs store named `Job` values in `~/.esdiag/jobs.yml`.
- Saved jobs reference known hosts and keystore secrets by name; they do not embed credentials.

## `setup`

If `[HOST]` is omitted, setup resolves output from:
- `ESDIAG_OUTPUT_URL`
- `ESDIAG_OUTPUT_APIKEY`
- `ESDIAG_OUTPUT_USERNAME`
- `ESDIAG_OUTPUT_PASSWORD`
- `ESDIAG_KIBANA_URL` (required for Kibana asset setup when host is omitted)

## `serve`

- Default port is `2501`.
- Output follows the same resolution rules as `process`.
- `--kibana <URL>` (or `ESDIAG_KIBANA_URL`) controls direct links in the UI.

## Cross-Cutting Options

- Use `--sources <path/to/sources.yml>` when diagnostics API selection must follow a custom or version-specific sources definition.
- Use metadata flags (`--account`, `--case`, `--opportunity`, `--user`) when reports need customer context.
