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

`bin/esdiag-lite.ps1` provides the same collection and send commands for
Windows PowerShell 5.1 or later.

## Download

Each [GitHub release](https://github.com/elastic/esdiag/releases) includes
both scripts and a SHA-256 checksum for each:

```sh
curl -fsSLO https://github.com/elastic/esdiag/releases/latest/download/esdiag-lite.sh
curl -fsSLO https://github.com/elastic/esdiag/releases/latest/download/esdiag-lite.sh.sha256
sha256sum --check esdiag-lite.sh.sha256
```

```powershell
$release = 'https://github.com/elastic/esdiag/releases/latest/download'
Invoke-WebRequest -UseBasicParsing "$release/esdiag-lite.ps1" -OutFile esdiag-lite.ps1
Invoke-WebRequest -UseBasicParsing "$release/esdiag-lite.ps1.sha256" -OutFile esdiag-lite.ps1.sha256
(Get-FileHash esdiag-lite.ps1 -Algorithm SHA256).Hash -eq (Get-Content esdiag-lite.ps1.sha256).Split(' ')[0]
```

On macOS, use `shasum -a 256 --check` instead of `sha256sum --check`.

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

## Send

Set an Elastic Upload Service ID when you want to omit it from the `send`
command:

```sh
export UPLOAD_ID="<upload-id>"
```

Send immediately after collection:

```sh
bin/esdiag-lite.sh collect --send="<upload-id>"
```

Or send an existing archive:

```sh
bin/esdiag-lite.sh send api-diagnostics-<timestamp>.zip
bin/esdiag-lite.sh send api-diagnostics-<timestamp>.zip "<upload-id>"
```

Uploads need `split` and one SHA-256 tool: `shasum`, `sha256sum`, or `openssl`.
The script creates 50 MB parts and skips parts that the service already has.
You can retry a failed send.

Setting `UPLOAD_ID` alone does not send collection output. Use `--send`.
Immediate sending needs ZIP output.

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
cargo run -p repo-tools --bin esdiag-lite-generate
cargo run -p repo-tools --bin esdiag-lite-generate --check
```

The generator updates and checks both `esdiag-lite.sh` and `esdiag-lite.ps1`.

## PowerShell

### Running a downloaded script

Create a local directory for the script and collected diagnostics, then open it
in PowerShell:

```powershell
$diagnosticsDirectory = Join-Path $env:USERPROFILE 'diagnostics'
New-Item -ItemType Directory -Path $diagnosticsDirectory -Force | Out-Null
Set-Location -LiteralPath $diagnosticsDirectory
```

Save the downloaded `esdiag-lite.ps1` in this directory. The examples below run
from this directory; a repository checkout is not required.

Check the effective policy:

```powershell
Get-ExecutionPolicy
Get-ExecutionPolicy -List
```

| Effective policy | Next step |
| --- | --- |
| `RemoteSigned` | Review the downloaded script and its source, then unblock that file as shown below. |
| `AllSigned` | Requires a valid signature from a trusted publisher. Unblocking does not satisfy this requirement. |
| `Restricted` | Blocks all scripts, even signed scripts. On a machine you manage, configure your account as shown below; otherwise use an approved native ESDiag executable or ask your administrator for an approved script policy. |

If all scopes are `Undefined`, Windows PowerShell defaults to `Restricted` on
Windows clients. On a machine you manage, enable scripts for your account once:

```powershell
Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned
```

This requires no administrator rights and persists for your account. It permits
local unsigned scripts; Internet-downloaded scripts still require a trusted
signature or explicit unblocking. It changes policy for your account, not just
for ESDiag Lite, and cannot override an organization policy.

Under `RemoteSigned`, review and trust the download, then unblock the file if
Windows marked it as coming from the Internet:

```powershell
Unblock-File -LiteralPath .\esdiag-lite.ps1
```

Unblocking is needed only once per downloaded copy. If you already unblocked
this copy before changing the execution policy, you do not need to repeat it.
Run the script directly:

```powershell
.\esdiag-lite.ps1 help
.\esdiag-lite.ps1 collect
```

Unblocking removes the file's Internet-origin marker; it does not verify the
publisher or change the execution policy. It requires no administrator rights
for a file you own. Repeat the review for each new download.

The error "running scripts is disabled on this system" occurs before ESDiag
Lite starts, so the script cannot display instructions or fix it itself.
Do not use `Bypass` launchers or machine-wide policy changes as installation
steps. Organization policies (`MachinePolicy` and `UserPolicy`) take precedence.
See Microsoft's [execution policy documentation](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_execution_policies?view=powershell-5.1).

### Commands

From your local directory containing the downloaded `esdiag-lite.ps1`:

```powershell
.\esdiag-lite.ps1 collect
.\esdiag-lite.ps1 collect --archive=none
.\esdiag-lite.ps1 send api-diagnostics-<timestamp>.zip
.\esdiag-lite.ps1 collect --send="<upload-id-or-url>"
```

PowerShell uses `Compress-Archive` for ZIP output. Use `--archive=none` when
that command is unavailable.

Set `ELASTIC_ES_URL` and `ELASTIC_ES_API_KEY` (or the basic authentication
pair) using `$env:VARIABLE = 'value'` before running the script. Uploads use
`Get-FileHash` and accept either an upload ID or the full Elastic Upload link.

```powershell
$env:WAIT_SECONDS = '60'
$env:COLLECTION_COUNT = '5'
.\esdiag-lite.ps1 watch --archive=none
```

Watch jobs save diagnostics in the directory from which you invoke the script.
The watch command exits with code 1 if any collection fails.
