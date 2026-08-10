## Context

`bin/min-diag.sh` directly lists fifteen Elasticsearch requests. Several of those APIs were introduced in later Elasticsearch releases or changed request paths across releases, while `assets/elasticsearch/sources.yml` already describes the relevant version rules, output names, extensions, and subdirectories. The script also uses `jq` to read `version.number`, which weakens its value in restricted environments.

The replacement is a checked-in, copyable shell artifact for operators who cannot deploy the ESDiag binary or a container. Runtime compatibility is Bash 3.2 plus `curl` and standard POSIX utilities. The default archive mode also requires `zip`, while `--archive=none` works without it. Generation is a maintainer workflow and may use the repository's Rust toolchain and existing YAML/semver dependencies.

This change does not introduce a Rust receiver, processor, exporter, or typestate transition. Its relevant state machine is the shell collection lifecycle:

```text
Configured
    |
    v
Output directory created
    |
    v
Root response saved as version.json
    |
    v
Version extracted and validated
    |
    v
Supported lite APIs collected; unsupported APIs skipped
    |
    v
diagnostic_manifest.json written
    |
    +-------------------------+
    |                         |
    v                         v
archive=zip                archive=none
ZIP created               directory retained
directory removed
```

Argument, environment, authentication, and dependency validation occurs before the output directory is created. Failure to create the output directory, fetch the root response, or validate the version prevents transition into version-dependent collection. An unsupported optional API is a skip within the collection stage, not a workflow failure. ZIP creation failure preserves the completed directory for recovery and returns an unsuccessful status.

## Goals / Non-Goals

**Goals:**

- Replace `bin/min-diag.sh` with the self-contained `bin/esdiag-lite.sh` artifact.
- Make `assets/elasticsearch/sources.yml` authoritative for lite-profile membership, request paths, output paths, and version boundaries.
- Generate transparent named Bash functions rather than interpret YAML or a general data table at runtime.
- Run on Bash 3.2 without `jq` or newer shell features.
- Preserve the existing `collect`, `watch`, interval configuration, directory naming, and processable-manifest behavior.
- Read the Elasticsearch URL and authentication only from Elastic CLI-style `ELASTIC_ES_*` environment variables and support API-key or username/password authentication.
- Produce a ZIP archive by default while allowing uncompressed directory output through `--archive=none`.
- Present `esdiag-lite.sh` as collection-only and preserve its ZIP or directory output as a valid input boundary for later `esdiag` processing.
- Detect unsupported APIs before making their HTTP requests.
- Detect stale generated content in automated tests.

**Non-Goals:**

- Reimplement a general JSON or NPM-semver parser in Bash.
- Generate collection behavior for sources not tagged `lite`.
- Add authentication modes beyond API key and username/password, retries, parallelism controls, archive formats beyond `zip` and `none`, upload targets beyond the Elastic Upload Service, or Kibana/Logstash collection.
- Change the ESDiag Rust CLI, Web UI, receiver, processor, or exporter behavior.
- Process, analyze, transform, export, or visualize diagnostics inside ESDiag Lite. Explicit forwarding of a completed ZIP archive to the Elastic Upload Service is allowed.
- Make the generated script POSIX `sh`; Bash 3.2 is the compatibility floor.

## Decisions

### `lite` tags define collection membership

The fifteen source keys currently requested by `min-diag.sh` will receive the comma-separated `lite` tag while retaining any existing tags. The generator will select sources through the existing tag semantics rather than carrying a second handwritten list.

The tagged keys are `version`, `alias`, `cluster_settings_defaults`, `data_stream`, `ilm_explain`, `ilm_policies`, `settings`, `indices_stats`, `licenses`, `nodes`, `nodes_stats`, `cluster_pending_tasks`, `searchable_snapshots_cache_stats`, `slm_policies`, and `tasks`.

Alternatives considered:

- Reusing `light` was rejected because the Rust light profile and the current helper-script set are not identical.
- Hard-coding source names in the generator was rejected because membership would still drift independently of the catalog.

### A Rust development tool updates a checked-in generated region

A small repository-side Rust generator will read the embedded Elasticsearch source model, select `lite` tags, and replace only marked generated regions in `bin/esdiag-lite.sh`. A `--check` mode will render in memory and fail if the checked-in script is stale. The tool should be exposed as a development target, such as a Cargo example, so it is not part of the deployed script or normal release binaries.

The existing NPM-to-Rust semver conversion will be factored into reusable source-resolution logic so runtime resolution and generation do not acquire divergent parsers. The generator will lower only the comparison operators it can represent with the Bash predicates and will fail on unsupported syntax.

Generating the full script was rejected because it would mix ordinary script maintenance with generated content. Runtime YAML parsing was rejected because it requires another dependency or a fragile YAML parser.

### Generate one function per tagged source

Each tagged source will produce a `get_api_<source>` function. The function contains the version predicate branches and delegates the actual request to the handwritten `get_api` primitive. The generator also emits the ordered `collect_lite_apis` function, preventing a second collection-membership list.

`version` is the bootstrap source. Its generated function saves the root response before any version predicates execute; collection orchestration then extracts the version and invokes the remaining generated functions. Other functions call a shared `skip_api` helper when no rule matches.

This structure was selected over a generated table because it is easier to inspect, troubleshoot, and test in a restricted environment. A single generated dispatcher was rejected because it concentrates unrelated APIs into one large function.

### Compare parsed numeric components

Handwritten Bash helpers will parse the detected version into major, minor, and patch integers and expose predicates corresponding to the source operators, including inclusive and exclusive bounds. Generated code will compose those predicates for bounded rules.

The implementation will avoid associative arrays, `mapfile`, namerefs, `wait -n`, case-glob version ranges, `sort -V`, and string ordering. Prerelease and build suffixes will be retained in the manifest value but removed before numeric comparison, matching existing source resolution behavior.

### Extract the stable root version field without jq

The root response will be saved as `version.json` and a narrowly scoped POSIX text extractor will locate the `version.number` string. The script will then require exactly one extracted value and validate its complete version shape before setting comparison state. Manifest generation will reuse the validated original value and write JSON with fixed literals and validated data, so no general JSON serializer is needed.

Embedding a general-purpose shell JSON parser was rejected as disproportionate and harder to audit. Requesting only `version.number` was rejected because `version.json` must retain the complete root response expected in a diagnostic bundle.

### Preserve bundle semantics while renaming the runner

The output directory pattern and existing API filenames will continue to come from the source catalog. The diagnostic manifest will retain the existing minimum mode and Elasticsearch diagnostic type, while its runner becomes `esdiag-lite`. Documentation will replace `min-diag.sh` commands and paths rather than maintain a compatibility wrapper.

`esdiag-lite.sh` terminates after collecting and optionally archiving raw API responses. It does not embed any diagnostic processing behavior. Script help and maintained documentation will label it as collection-only, describe the generated ZIP or directory as an input bundle, and show the handoff to `esdiag process` for analysis and export.

### Configure connection and authentication through the environment

The script will remove editable `URL` and `APIKEY` values from its configuration section. Instead, it will require a non-empty `ELASTIC_ES_URL` and select authentication from these environment variables:

- `ELASTIC_ES_API_KEY`: encoded Elasticsearch API key sent with the `Authorization: ApiKey` scheme.
- `ELASTIC_ES_USERNAME`: Elasticsearch username for HTTP basic authentication.
- `ELASTIC_ES_PASSWORD`: Elasticsearch password paired with `ELASTIC_ES_USERNAME`.

A non-empty `ELASTIC_ES_API_KEY` selects API-key authentication. If it is absent or empty, both username and password must be non-empty to select basic authentication. When API key and username are both configured, API-key authentication takes precedence and the username/password values are ignored rather than validated. If neither complete mode is available, validation fails before collection.

The shared handwritten `get_api` primitive will add only the selected authentication arguments to every request, including the bootstrap version request. Help and documentation will describe the environment variables without printing their values, and logging will never include credential contents.

Environment configuration was selected over command arguments or editable script variables to match Elastic CLI naming, avoid storing credentials in copied script files, and allow the same artifact to move between environments. Anonymous authentication remains out of scope because the existing helper requires credentials.

### Select archive output explicitly

The handwritten argument parser will accept `--archive=zip` and `--archive=none` after the `collect` or `watch` command. Omitting the option is equivalent to `--archive=zip`. Unknown values will fail argument validation so additional formats can be introduced later without ambiguous behavior.

When ZIP is selected, dependency validation will use `command -v zip` before collection begins. If unavailable, the script will exit with exactly:

```text
No zip executable found, run with --archive=none to skip archive creation
```

After a completed collection, ZIP mode will archive the diagnostic directory's contents at the archive root into `api-diagnostics-<timestamp>.zip`. The directory will be removed only after successful archive creation. If `zip` fails, the completed directory will remain available and the command will return unsuccessfully. `none` mode will skip the dependency check and archive step and return the directory as the final artifact.

A `tar.gz` fallback was rejected because ESDiag archive receivers currently accept ZIP and because neither `tar` nor `gzip` is guaranteed by the runtime contract. Requiring ZIP unconditionally was rejected because restricted environments must retain a no-compression path.

### Enforce one shell style across handwritten and generated code

The complete checked-in `bin/esdiag-lite.sh`, including generated regions, will be validated with Bash syntax checking and ShellCheck and formatted with the repository's required `shfmt` options:

```bash
script=bin/esdiag-lite.sh
bash -n "$script"
shellcheck "$script"
shfmt -d -i 2 -ci -bn "$script"
```

The generator must emit code that already conforms to this style so regeneration produces no `shfmt` diff. ShellCheck and `shfmt` are maintainer and CI dependencies only; they are not required in the restricted environment that executes the script.

### Provide a Windows PowerShell counterpart

`bin/esdiag-lite.ps1` will provide the same operator-facing collection contract on Windows PowerShell 5.1 and newer: `collect`, `watch`, `upload`, `--archive=zip|none`, `--upload=<id>`, `ELASTIC_ES_*` authentication, `UPLOAD_*` configuration, version-aware API selection, and the same processable bundle layout. The filename follows PowerShell's standard `.ps1` convention; documentation will show invocation with `powershell -File bin/esdiag-lite.ps1` (or a PowerShell host's equivalent).

A repository-side generator will render named `Get-Api<Source>` functions and the collection sequence into marked regions in both script artifacts from the shared `lite` source definitions. Handwritten PowerShell code will own argument parsing, version predicates, HTTP requests, manifest generation, archiving, and upload transport. This preserves a single authoritative API catalog without requiring a YAML parser or generator tooling on the target Windows host.

PowerShell built-ins will replace Unix runtime utilities where practical: `Invoke-WebRequest` for requests, `Compress-Archive` for ZIP creation, `Get-FileHash` for SHA-256, and .NET stream APIs for 50,000,000-byte upload parts. ZIP remains an optional mode; the script must retain uncompressed collection when `Compress-Archive` is unavailable. No Bash, `curl`, `jq`, external `zip`, `split`, or Unix compatibility layer will be required to run the PowerShell artifact.

The PowerShell script will be formatted and statically checked with repository-defined PowerShell tooling where available, and its generated region will participate in the existing drift check. Behavior tests will mock HTTP and archive/upload boundaries so they run without a live cluster or upload service.

## Risks / Trade-offs

- [The version extractor is not a general JSON parser] -> Limit it to the stable Elasticsearch root schema, validate exactly one complete version, and fail before versioned requests when extraction is ambiguous.
- [Bash 3.2 behaves differently from current Bash releases] -> Avoid post-3.2 constructs and run syntax and behavior tests against Bash 3.2 in addition to the host shell where CI permits.
- [Generated functions can become stale] -> Provide deterministic generation and `--check` mode in automated tests.
- [Future source rules may use richer NPM semver syntax] -> Reject rules the generator cannot lower and include the source name and expression in the error.
- [Removing `min-diag.sh` breaks existing paths] -> Announce the rename in the changelog and update all repository documentation in the same change.
- [A source can have overlapping or incomplete version rules] -> Test generated boundary behavior against the Rust source resolver and require at most one matching generated branch for each evaluated version.
- [The default archive tool may be absent] -> Check before collection, emit the prescribed remediation message, and allow explicit `--archive=none` operation without `zip`.
- [ZIP creation can fail after collection] -> Preserve the complete directory unless archive creation succeeds, then return a failure that can be retried manually.
- [Environment credentials can leak through diagnostics or logs] -> Never echo credential values, include them in generated files, or render them in help output; pass only the selected mode to `curl`.
- [Partially configured basic authentication can produce confusing requests] -> Validate the username/password pair before collection whenever API-key authentication is not selected.
- [Generated Bash can bypass ordinary style review] -> Run Bash syntax validation, ShellCheck, and the prescribed `shfmt` diff check against the entire regenerated artifact.

## Migration Plan

1. Add `lite` tags without changing existing source paths or Rust collection profiles.
2. Add the generator, generated regions, and parity/drift tests.
3. Add `bin/esdiag-lite.sh`, verify both authentication modes, and verify its ZIP and uncompressed directory outputs can be processed by ESDiag.
4. Replace documentation references and remove `bin/min-diag.sh`.
5. Record the path rename and dependency reduction in `CHANGELOG.md`.

Rollback consists of restoring `bin/min-diag.sh` and its documentation. The source tags are additive metadata and may remain without affecting existing collection modes.

## Open Questions

None.
