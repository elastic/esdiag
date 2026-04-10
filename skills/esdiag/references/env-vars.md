# ESDiag Environment Variables

Use these variables when configuring `esdiag` without saved hosts, or to supply credentials and settings non-interactively.

## Path Overrides

| Variable | Default | Purpose |
|---|---|---|
| `ESDIAG_HOME` | `~/.esdiag` | Base directory for all esdiag config and data files |
| `ESDIAG_HOSTS` | `$ESDIAG_HOME/hosts.yml` | Override path to the saved-hosts file |
| `ESDIAG_KEYSTORE` | `$ESDIAG_HOME/secrets.yml` | Override path to the encrypted keystore file |

## Output Target (`process`, `setup`, `serve`)

| Variable | Purpose |
|---|---|
| `ESDIAG_OUTPUT_URL` | Elasticsearch output URL |
| `ESDIAG_OUTPUT_APIKEY` | API key for output cluster |
| `ESDIAG_OUTPUT_USERNAME` | Basic auth username for output cluster |
| `ESDIAG_OUTPUT_PASSWORD` | Basic auth password for output cluster |
| `ESDIAG_KIBANA_URL` | Kibana URL — required for Kibana asset setup when `[HOST]` is omitted from `setup`, and to generate dashboard links in `serve` |
| `ESDIAG_KIBANA_SPACE` | Kibana space ID to use when constructing dashboard links. Defaults to `esdiag` when unset; set it to an empty value to omit the `/s/<space>` suffix |

## Keystore

| Variable | Purpose |
|---|---|
| `ESDIAG_KEYSTORE_PASSWORD` | Keystore password for non-interactive operations; suppresses the password prompt in `keystore add/update/remove/unlock/password` |

## Report / Identity

| Variable | Purpose |
|---|---|
| `ESDIAG_USER` | Default user email attached to report metadata; overridden by `--user` flag |

## Server (`serve`)

| Variable | Default | Purpose |
|---|---|---|
| `ESDIAG_MODE` | `user` | Runtime mode: `user` (single-user) or `service` (multi-user) |
| `ESDIAG_PORT` | `2501` | Port the upload service listens on; overridden by `--port` flag |

## Performance Tuning

| Variable | Default | Purpose |
|---|---|---|
| `ESDIAG_ES_BULK_SIZE` | `5000` | Number of documents per Elasticsearch bulk request |
| `ESDIAG_ES_WORKERS` | `4` | Number of parallel worker threads for export |
| `ESDIAG_OUTPUT_TASK_LIMIT` | — | Max concurrent tasks when sending to Elasticsearch |
| `ESDIAG_REQUEST_TIMEOUT_MS` | — | HTTP request timeout in milliseconds |
| `ESDIAG_EXPORT_RETRY_MAX` | — | Maximum number of export retry attempts |
| `ESDIAG_EXPORT_RETRY_INITIAL_MS` | — | Initial retry backoff in milliseconds |
| `ESDIAG_EXPORT_RETRY_MAX_MS` | — | Maximum retry backoff ceiling in milliseconds |

## Logging

| Variable | Default | Purpose |
|---|---|---|
| `LOG_LEVEL` | `info` | Log verbosity: `error`, `warn`, `info`, `debug`, `trace` |
