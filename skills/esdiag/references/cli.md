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
Usage: esdiag host [OPTIONS] <NAME> [APP] [URL]
```

Key options:
- `--accept-invalid-certs`
- `-a, --apikey <APIKEY>`
- `-u, --username <USERNAME>`
- `-p, --password <PASSWORD>`
- `-n, --nosave`
- `--sources <SOURCES>`

## process

```text
Usage: esdiag process [OPTIONS] <INPUT> [OUTPUT]
```

- `<INPUT>`: archive, directory, known host, or Elastic uploader URL.
- `[OUTPUT]`: known host, file, `-` for stdout, or environment fallback.

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
