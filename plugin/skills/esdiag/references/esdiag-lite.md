# ESDiag Lite

ESDiag Lite collects Elasticsearch API diagnostics without a native binary or
container runtime.

- Linux, macOS, and WSL use `esdiag-lite.sh`.
- Native Windows uses `esdiag-lite.ps1`.

The user sets `ELASTIC_ES_URL` and authentication in their terminal. Do not ask
for those values in chat.

## Collect

Collect a ZIP archive:

```sh
esdiag-lite.sh collect
```

```powershell
powershell -File esdiag-lite.ps1 collect
```

Use `--archive=none` for an unpacked directory. ESDiag Lite collects
Elasticsearch only. Kibana and Logstash collection are unsupported.

## Share

Upload while collecting:

```sh
esdiag-lite.sh collect --upload="<upload-id>"
```

```powershell
powershell -File esdiag-lite.ps1 collect --upload="<upload-id>"
```

Upload an existing ZIP archive:

```sh
esdiag-lite.sh upload api-diagnostics-<timestamp>.zip "<upload-id>"
```

```powershell
powershell -File esdiag-lite.ps1 upload api-diagnostics-<timestamp>.zip "<upload-id>"
```

An upload ID is required. Directory output cannot be uploaded.

## Process

Processing is unsupported. Move the archive or directory to a host with a
native `esdiag` binary, a supported `esdiag-local exec` installation, or a
hosted ESDiag service.

## Analyze

Analysis and Agent Builder are unsupported. ESDiag Lite has no Kibana
connection, web UI, processing output, or conversation state.

## Environment variables

Both Lite scripts read these values:

| Variable | Purpose |
|---|---|
| `ELASTIC_ES_URL` | Required Elasticsearch URL. |
| `ELASTIC_ES_API_KEY` | Elasticsearch API key. Takes precedence over basic authentication. |
| `ELASTIC_ES_USERNAME` | Basic-auth username. Requires `ELASTIC_ES_PASSWORD`. |
| `ELASTIC_ES_PASSWORD` | Basic-auth password. Requires `ELASTIC_ES_USERNAME`. |
| `UPLOAD_HOST` | Elastic Upload Service URL. Defaults to `https://upload.elastic.co`. |
| `UPLOAD_ID` | Upload ID used when a command omits one. |

`esdiag-lite.sh` also reads `LOG_LEVEL`. The PowerShell script does not.
The user sets credential values in their own shell.
