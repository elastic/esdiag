# ESDiag CLI Reference

Use this file for exact command syntax when building or checking command lines.

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
```

## collect

```text
Usage: esdiag collect [OPTIONS] <HOST> <OUTPUT>
```

- `<HOST>`: known Elasticsearch host to collect from.
- `<OUTPUT>`: existing directory where a diagnostic directory is created.

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

## serve

```text
Usage: esdiag serve [OPTIONS] [OUTPUT]
```

Key options:
- `-p, --port <PORT>` (default `2501`)
- `--kibana <KIBANA>`

## setup

```text
Usage: esdiag setup [OPTIONS] [HOST]
```

If `[HOST]` is omitted, resolve output from:
- `ESDIAG_OUTPUT_URL`
- `ESDIAG_OUTPUT_APIKEY`
- `ESDIAG_OUTPUT_USERNAME`
- `ESDIAG_OUTPUT_PASSWORD`
