# ESDiag CLI Reference

Use this file as a concise command map. For complete and version-accurate options, run `esdiag --help` and `esdiag <command> --help`.

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
- `add <NAME> <APP> <URL>`
- `update <NAME>`
- `remove <NAME>`
- `list`
- `auth <NAME>`

Shared host auth/update options:
- `--accept-invalid-certs <true|false>`
- `-k, --apikey <APIKEY>`
- `-u, --username <USERNAME>`
- `-p, --password <PASSWORD>`
- `--secret <SECRET>`
- `--roles <ROLES>`

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

Report options:
- `-a, --account <ACCOUNT>`
- `-c, --case <CASE>`
- `-o, --opportunity <OPPORTUNITY>`
- `-u, --user <USER>`
- `--sources <SOURCES>`

## serve

```text
Usage: esdiag serve [OPTIONS] [OUTPUT]
```

Key options:
- `-p, --port <PORT>` (default `2501`)
- `--kibana <KIBANA>`
- `--sources <SOURCES>`

## setup

```text
Usage: esdiag setup [OPTIONS] [HOST]
```

Key options:
- `--sources <SOURCES>`

If `[HOST]` is omitted, resolve output from:
- `ESDIAG_OUTPUT_URL`
- `ESDIAG_OUTPUT_APIKEY`
- `ESDIAG_OUTPUT_USERNAME`
- `ESDIAG_OUTPUT_PASSWORD`
- `ESDIAG_KIBANA_URL` (required for Kibana asset setup when host is omitted)
