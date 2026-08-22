---
type: Guide
title: esdiag-lite
description: Collect Elasticsearch diagnostics without installing the ESDiag binary.
tags: [bin, collection, diagnostics]
---

# esdiag-lite

`bin/esdiag-lite.sh` collects Elasticsearch API responses into a diagnostic
archive. It does not process, analyze, export, or visualize the data. Pass its
archive or directory to `esdiag process` when ESDiag is available elsewhere.

The Bash script needs Bash 3.2 or later, `curl`, and standard POSIX tools. ZIP
output also needs `zip`. It does not need ESDiag, a container runtime, `jq`,
`yq`, Python, or Rust.

`bin/esdiag-lite.ps1` provides the same collection and upload commands for
Windows PowerShell 5.1 or later.

## Configure access

Set the Elasticsearch URL and an API key:

```sh
export ELASTIC_ES_URL="https://elasticsearch.example:9200"
export ELASTIC_ES_API_KEY="<encoded-api-key>"
```

Or use basic authentication:

```sh
export ELASTIC_ES_URL="https://elasticsearch.example:9200"
export ELASTIC_ES_USERNAME="diagnostic-reader"
export ELASTIC_ES_PASSWORD="<password>"
```

`ELASTIC_ES_URL` is required. An API key wins over username and password. The
script does not print or write these values.

## Collect

```sh
bin/esdiag-lite.sh collect
```

This creates `api-diagnostics-<timestamp>.zip`. Use `--archive=none` to keep
the uncompressed directory instead:

```sh
bin/esdiag-lite.sh collect --archive=none
```

Only `zip` and `none` are valid archive formats. The ZIP source directory is
removed only after archive creation succeeds.

For repeated collection:

```sh
WAIT_SECONDS=60 COLLECTION_COUNT=5 \
  bin/esdiag-lite.sh watch --archive=none
```

## Upload

Set an upload ID when you want to omit it from the `upload` command:

```sh
export UPLOAD_ID="<upload-id>"
```

Upload immediately after collection:

```sh
bin/esdiag-lite.sh collect --upload="<upload-id>"
```

Or upload an existing archive:

```sh
bin/esdiag-lite.sh upload api-diagnostics-<timestamp>.zip
bin/esdiag-lite.sh upload api-diagnostics-<timestamp>.zip "<upload-id>"
```

Uploads need `split` and one SHA-256 tool: `shasum`, `sha256sum`, or `openssl`.
The script creates 50 MB parts and skips parts that the service already has.
You can retry a failed upload.

Setting `UPLOAD_ID` alone does not upload collection output. Use `--upload`.
Immediate upload needs ZIP output.

## Process the output

```sh
esdiag process api-diagnostics-<timestamp>.zip diagnostics-output
esdiag process api-diagnostics-<timestamp> diagnostics-output
```

`diagnostics-output` must be a saved ESDiag output host.

## Source definitions

The script reads `version.json`, then selects Elasticsearch APIs that support
that version. Unsupported APIs are recorded as skipped.

Maintainers generate its API functions from `lite`-tagged entries in
`assets/elasticsearch/sources.yml`:

```sh
cargo run --bin esdiag-lite-generate
cargo run --bin esdiag-lite-generate --check
```

The generator updates and checks both `esdiag-lite.sh` and `esdiag-lite.ps1`.

## PowerShell

```powershell
powershell -File bin/esdiag-lite.ps1 collect
powershell -File bin/esdiag-lite.ps1 collect --archive=none
powershell -File bin/esdiag-lite.ps1 upload api-diagnostics-<timestamp>.zip
```

PowerShell uses `Compress-Archive` for ZIP output. Use `--archive=none` when
that command is unavailable.
