## Why

`bin/min-diag.sh` is useful in high-security Elasticsearch environments where the ESDiag binary or container cannot be deployed, but its hard-coded API paths are not compatible with every supported Elasticsearch version. A generated, self-contained replacement can preserve the script's portability while deriving its API selection from the same version-aware source catalog as ESDiag.

## What Changes

- Add `bin/esdiag-lite.sh` as a self-contained Elasticsearch diagnostic collection script compatible with Bash 3.2.
- Add `bin/esdiag-lite.ps1` as a self-contained Windows PowerShell equivalent of the Bash collector.
- Document `esdiag-lite.sh` as a collection-only utility whose ZIP or directory output can be supplied to `esdiag` for diagnostic processing.
- Generate named `get_api_<name>` functions and their collection order from `assets/elasticsearch/sources.yml`.
- Add the `lite` tag to every Elasticsearch source currently collected by `bin/min-diag.sh`, making the source catalog authoritative for lite-profile membership and version-specific request paths.
- Detect and compare the target Elasticsearch version using Bash 3.2-compatible numeric predicates, selecting exactly one supported request path or logging that an unavailable API was skipped.
- Remove the runtime `jq` dependency by extracting and validating `version.number` from the saved Elasticsearch root response and reusing that value when writing the diagnostic manifest.
- Replace in-script Elasticsearch URL and authentication configuration with `ELASTIC_ES_URL`, `ELASTIC_ES_API_KEY`, `ELASTIC_ES_USERNAME`, and `ELASTIC_ES_PASSWORD` environment variables.
- Support API-key and username/password authentication, with `ELASTIC_ES_API_KEY` taking precedence whenever both authentication modes are configured.
- Add `--archive=<format>` with `zip` and `none` formats. ZIP is the default and produces a directly processable archive; `none` preserves the collected diagnostic directory without compression.
- Check for `zip` only when ZIP output is selected and direct operators without it to rerun with `--archive=none`.
- Support explicit optional forwarding of ZIP output to the Elastic Upload Service without adding diagnostic processing to the utility.
- Add generation drift checks and boundary-version tests that compare generated behavior with the source definitions.
- Enforce shell hygiene across handwritten and generated code with Bash syntax validation, ShellCheck, and `shfmt -d -i 2 -ci -bn`.
- Update helper-script documentation and user-visible release notes.
- **BREAKING**: Replace `bin/min-diag.sh` with `bin/esdiag-lite.sh`; callers using the former path must adopt the new script name.

## Capabilities

### New Capabilities

- `portable-lite-collection`: Version-aware, generated, collection-only Elasticsearch diagnostics and optional ZIP output through a self-contained Bash 3.2 script with no `jq` dependency, producing input suitable for later ESDiag processing.
- `windows-lite-collection`: Version-aware, generated, collection-only Elasticsearch diagnostics through a self-contained Windows PowerShell script with output compatible with the Bash collector and later ESDiag processing.

### Modified Capabilities

None.

## Impact

- Affects `bin/min-diag.sh`, the new `bin/esdiag-lite.sh` and `bin/esdiag-lite.ps1`, `assets/elasticsearch/sources.yml`, repository-side generation tooling, platform-specific lint/format checks, tests, `docs/bin/`, `bin/readme.md`, and `CHANGELOG.md`.
- Targets Elasticsearch collection only.
- Does not change the ESDiag Rust CLI, Web UI, or core processing behavior, but the generated bundle must remain compatible with existing Elasticsearch diagnostic processing.
- Does not process, analyze, transform, or export diagnostic data within ESDiag Lite; those operations remain in `esdiag`. Explicit Elastic Upload Service forwarding is the sole supported output transmission.
- Deployed environments configure the Elasticsearch endpoint and credentials through `ELASTIC_ES_*` environment variables and require Bash 3.2 or newer, `curl`, and standard POSIX utilities. The default ZIP archive mode additionally requires `zip`; environments without it can use `--archive=none`. The script does not require `jq`, `yq`, Python, Rust, or the ESDiag binary.
