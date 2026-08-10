## Purpose

Define the portable Bash ESDiag Lite collector and its generated, version-aware API behavior.

## Requirements

### Requirement: Portable ESDiag Lite Artifact
The project SHALL provide `bin/esdiag-lite.sh` as a self-contained Elasticsearch diagnostic collection script that runs under Bash 3.2 or newer. Uncompressed collection MUST require only Bash, `curl`, and standard POSIX utilities. ZIP output additionally requires a runtime `zip` executable. The deployed script MUST NOT require `jq`, `yq`, Python, Rust, the ESDiag binary, or a container runtime.

#### Scenario: Collect in a restricted environment
- **GIVEN** a host with Bash 3.2, `curl`, and standard POSIX utilities
- **AND** valid Elasticsearch environment configuration
- **WHEN** the operator runs `bin/esdiag-lite.sh collect --archive=none`
- **THEN** the script collects a processable Elasticsearch API diagnostic without invoking `jq`, `yq`, Python, Rust, the ESDiag binary, or a container runtime

#### Scenario: Use the periodic collection interface
- **GIVEN** valid Elasticsearch environment configuration, `WAIT_SECONDS`, and `COLLECTION_COUNT`
- **WHEN** the operator runs `bin/esdiag-lite.sh watch`
- **THEN** the script starts the configured number of collections at the configured interval

### Requirement: Environment-Based Elasticsearch Configuration
The script SHALL read the Elasticsearch endpoint and credentials from `ELASTIC_ES_URL`, `ELASTIC_ES_API_KEY`, `ELASTIC_ES_USERNAME`, and `ELASTIC_ES_PASSWORD`. It MUST require a non-empty `ELASTIC_ES_URL` and MUST NOT require operators to edit or store the endpoint or credentials in the script file.

#### Scenario: Configure API-key authentication
- **GIVEN** `ELASTIC_ES_URL` and `ELASTIC_ES_API_KEY` are non-empty
- **WHEN** collection begins
- **THEN** every Elasticsearch request uses `ELASTIC_ES_URL`
- **AND** sends `ELASTIC_ES_API_KEY` with the `Authorization: ApiKey` authentication scheme

#### Scenario: Configure username and password authentication
- **GIVEN** `ELASTIC_ES_URL`, `ELASTIC_ES_USERNAME`, and `ELASTIC_ES_PASSWORD` are non-empty
- **AND** `ELASTIC_ES_API_KEY` is absent or empty
- **WHEN** collection begins
- **THEN** every Elasticsearch request uses HTTP basic authentication with the configured username and password

#### Scenario: API key takes precedence
- **GIVEN** `ELASTIC_ES_API_KEY` and `ELASTIC_ES_USERNAME` are both non-empty
- **WHEN** authentication is selected
- **THEN** the script uses API-key authentication
- **AND** ignores the username and password configuration

#### Scenario: Reject incomplete authentication
- **GIVEN** `ELASTIC_ES_API_KEY` is absent or empty
- **AND** either `ELASTIC_ES_USERNAME` or `ELASTIC_ES_PASSWORD` is absent or empty
- **WHEN** environment configuration is validated
- **THEN** the script exits unsuccessfully before collection begins
- **AND** reports that a complete API key or username/password pair is required

#### Scenario: Do not expose credentials
- **GIVEN** any supported authentication mode is configured
- **WHEN** the script displays help, validates configuration, logs requests, or writes diagnostic files
- **THEN** it does not print or persist the API key, username, or password values

### Requirement: Selectable Archive Output
The `collect` and `watch` commands SHALL accept `--archive=<format>` with exactly the formats `zip` and `none`. The default format SHALL be `zip`. ZIP mode SHALL create an ESDiag-processable `api-diagnostics-<timestamp>.zip`; none mode SHALL retain the `api-diagnostics-<timestamp>` directory without compression.

#### Scenario: Use the default ZIP format
- **GIVEN** a `zip` executable is available
- **WHEN** the operator runs `bin/esdiag-lite.sh collect` without an archive option
- **THEN** the script behaves as if `--archive=zip` was provided
- **AND** produces `api-diagnostics-<timestamp>.zip` with diagnostic files at the archive root
- **AND** removes the source directory only after ZIP creation succeeds

#### Scenario: Request uncompressed output
- **GIVEN** `zip` is not installed
- **WHEN** the operator runs `bin/esdiag-lite.sh collect --archive=none`
- **THEN** the script does not check for or invoke `zip`
- **AND** preserves the completed `api-diagnostics-<timestamp>` directory as the final output

#### Scenario: ZIP executable is unavailable
- **GIVEN** ZIP output is selected explicitly or by default
- **AND** no `zip` executable can be found
- **WHEN** command dependencies are validated
- **THEN** the script exits unsuccessfully before collection begins
- **AND** emits exactly `No zip executable found, run with --archive=none to skip archive creation`

#### Scenario: Reject an unknown archive format
- **GIVEN** the operator provides an archive format other than `zip` or `none`
- **WHEN** arguments are validated
- **THEN** the script exits unsuccessfully and displays archive option usage

#### Scenario: ZIP creation fails after collection
- **GIVEN** ZIP output is selected and diagnostic collection completes
- **WHEN** the `zip` command fails
- **THEN** the script exits unsuccessfully
- **AND** retains the completed diagnostic directory

### Requirement: Lite Source Membership
Every Elasticsearch source currently collected by `bin/min-diag.sh` SHALL carry the `lite` tag in `assets/elasticsearch/sources.yml`. The tagged set SHALL contain `version`, `alias`, `cluster_settings_defaults`, `data_stream`, `ilm_explain`, `ilm_policies`, `settings`, `indices_stats`, `licenses`, `nodes`, `nodes_stats`, `cluster_pending_tasks`, `searchable_snapshots_cache_stats`, `slm_policies`, and `tasks`.

#### Scenario: Generate the lite source set
- **GIVEN** the Elasticsearch source catalog contains tagged and untagged sources
- **WHEN** the repository generator builds the ESDiag Lite API functions
- **THEN** it generates collection behavior for every source tagged `lite`
- **AND** it does not generate collection behavior for untagged sources

### Requirement: Generated Named API Functions
The repository generator SHALL emit a named `get_api_<name>` Bash function for every `lite` source and SHALL emit the function that invokes the complete generated collection sequence. Generated functions MUST derive their request paths, output paths, and supported version rules from `assets/elasticsearch/sources.yml`.

#### Scenario: Generate a version-dependent source
- **GIVEN** a `lite` source has different request paths before and after an Elasticsearch version boundary
- **WHEN** the generator renders its named API function
- **THEN** the function uses Bash version predicates to select the request path for the detected cluster version
- **AND** every selected branch delegates HTTP execution to the shared handwritten `get_api` function

#### Scenario: Generate source output placement
- **GIVEN** a `lite` source specifies a subdirectory or non-default extension
- **WHEN** its named function is generated
- **THEN** the output path reflects the source's `subdir`, name, and extension configuration

### Requirement: Bootstrap Version Detection Without jq
The script SHALL fetch the Elasticsearch root response into `version.json` before invoking version-dependent API functions. It SHALL extract `version.number` using Bash 3.2-compatible logic and standard POSIX utilities, strictly validate the extracted value, and reuse it for endpoint selection and manifest generation.

#### Scenario: Detect a release version
- **GIVEN** the saved Elasticsearch root response contains a valid `version.number`
- **WHEN** collection initializes
- **THEN** the script parses numeric major, minor, and patch components
- **AND** proceeds to version-dependent API collection

#### Scenario: Detect a prerelease version
- **GIVEN** the root response reports a version such as `9.0.0-SNAPSHOT`
- **WHEN** collection initializes
- **THEN** the script preserves the reported version for the manifest
- **AND** compares API rules using the normalized `9.0.0` numeric components

#### Scenario: Reject an unreadable version
- **GIVEN** the root request fails or its saved response does not contain one valid Elasticsearch version number
- **WHEN** collection initializes
- **THEN** the script logs an error and exits unsuccessfully before invoking version-dependent APIs

### Requirement: Bash 3.2 Version Predicates
The script SHALL compare Elasticsearch major, minor, and patch components numerically through Bash 3.2-compatible predicate functions. Generated logic MUST NOT depend on associative arrays, `sort -V`, lexicographic version ordering, or Bash features introduced after 3.2.

#### Scenario: Compare double-digit minor versions
- **GIVEN** the detected Elasticsearch version is `7.10.0`
- **WHEN** a generated function evaluates a boundary at `7.7.0`
- **THEN** the predicates treat `7.10.0` as newer than `7.7.0`

#### Scenario: Evaluate a bounded source rule
- **GIVEN** a source rule requires `>= 6.6.0 < 7.7.0`
- **WHEN** its generated function runs against Elasticsearch `7.6.2`
- **THEN** the function selects that rule's request path

### Requirement: Unsupported Lite APIs Are Skipped
When no version rule for a tagged source matches the detected Elasticsearch version, the script SHALL log that the API is unsupported and skip it without failing the remaining collection. A matching source SHALL be requested exactly once.

#### Scenario: Skip an API that predates its introduction
- **GIVEN** a lite source is supported only on Elasticsearch `7.13.0` or newer
- **AND** the detected cluster version is `7.12.1`
- **WHEN** its generated function executes
- **THEN** the script records an informational skip
- **AND** does not request that API
- **AND** continues collecting the remaining lite APIs

#### Scenario: Collect one matching version branch
- **GIVEN** a lite source has multiple non-overlapping version rules
- **WHEN** exactly one rule matches the detected cluster version
- **THEN** the script requests only the endpoint associated with that rule

### Requirement: Processable Diagnostic Output
Each successful collection SHALL produce either an `api-diagnostics-<timestamp>.zip` archive or an uncompressed `api-diagnostics-<timestamp>` directory, according to the selected archive format, containing the collected API files and a valid `diagnostic_manifest.json`. The manifest SHALL identify Elasticsearch as the product, preserve the existing minimum diagnostic mode, identify `esdiag-lite` as the runner, and contain the detected cluster version.

#### Scenario: Process an ESDiag Lite bundle
- **GIVEN** `bin/esdiag-lite.sh collect` completes successfully in either archive mode
- **WHEN** ESDiag processes the resulting ZIP archive or directory
- **THEN** it recognizes the output as an Elasticsearch diagnostic
- **AND** processes the available collected API files without requiring files for version-unsupported skipped APIs

### Requirement: Collection-Only Utility Boundary
The script and its documentation SHALL identify `esdiag-lite.sh` as a collection-only utility. The script MUST NOT process, analyze, transform, export, or visualize collected diagnostics. It MAY explicitly forward a completed ZIP archive to the Elastic Upload Service, but that transport MUST NOT modify the diagnostic content or perform ESDiag processing. Its ZIP archive or directory output SHALL be suitable as input to `esdiag` diagnostic processing.

#### Scenario: Complete lite collection without processing
- **GIVEN** valid environment and archive configuration
- **WHEN** `esdiag-lite.sh collect` completes
- **THEN** it outputs raw collected diagnostic files and a diagnostic manifest
- **AND** does not create processed diagnostic documents or export them to an Elasticsearch output cluster

#### Scenario: Hand collected output to ESDiag
- **GIVEN** `esdiag-lite.sh` produced a ZIP archive or uncompressed diagnostic directory
- **WHEN** an operator supplies that output to `esdiag process`
- **THEN** `esdiag` accepts it as diagnostic input for processing

#### Scenario: Read collection-only documentation
- **GIVEN** an operator views the script help or maintained ESDiag Lite documentation
- **WHEN** collection and processing responsibilities are described
- **THEN** the documentation explicitly states that `esdiag-lite.sh` only collects diagnostics
- **AND** directs the operator to use `esdiag` to process the collected output

### Requirement: Reproducible Generated Content
The project SHALL provide a repository-side generation command and an automated check that fails when the checked-in generated region of `bin/esdiag-lite.sh` differs from the output derived from the current `lite` sources.

#### Scenario: Source definitions change without regeneration
- **GIVEN** a maintainer changes a `lite` source path, output configuration, tag, or version rule
- **AND** does not regenerate `bin/esdiag-lite.sh`
- **WHEN** the generation drift check runs
- **THEN** the check fails and identifies the generated artifact as stale

#### Scenario: Generator encounters an unsupported rule form
- **GIVEN** a tagged source uses a semver expression the generator cannot translate into supported Bash predicates
- **WHEN** generation runs
- **THEN** generation fails with the source name and unsupported rule rather than emitting incomplete behavior

### Requirement: Shell Source Hygiene
The complete `bin/esdiag-lite.sh` artifact, including handwritten and generated regions, SHALL pass Bash syntax validation and ShellCheck. It SHALL conform to the output enforced by `shfmt -d -i 2 -ci -bn`.

#### Scenario: Validate the checked-in script
- **GIVEN** `bin/esdiag-lite.sh` has been generated
- **WHEN** repository validation runs `bash -n` and ShellCheck against the script
- **THEN** both commands complete successfully without findings

#### Scenario: Check required shell formatting
- **GIVEN** `bin/esdiag-lite.sh` has been generated
- **WHEN** repository validation runs `shfmt -d -i 2 -ci -bn bin/esdiag-lite.sh`
- **THEN** `shfmt` reports no diff

#### Scenario: Run the script in a restricted environment
- **GIVEN** the checked-in script passed development validation
- **WHEN** an operator runs it in the target environment
- **THEN** ShellCheck and `shfmt` are not required at runtime

### Requirement: Helper Script Rename
The project SHALL replace the `min-diag.sh` helper and its documentation references with `esdiag-lite.sh`.

#### Scenario: Follow the documented lite workflow
- **GIVEN** an operator follows the maintained helper-script documentation
- **WHEN** they locate and invoke the portable diagnostic helper
- **THEN** all commands and paths refer to `esdiag-lite.sh`
