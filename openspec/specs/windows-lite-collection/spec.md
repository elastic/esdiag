## Purpose

Define the native Windows PowerShell ESDiag Lite collector and its generated, version-aware API behavior.

## Requirements

### Requirement: Windows PowerShell ESDiag Lite Artifact
The project SHALL provide `bin/esdiag-lite.ps1` as a self-contained, version-aware Elasticsearch diagnostic collection utility for Windows PowerShell 5.1 and newer. The script SHALL require PowerShell and its built-in .NET runtime only; it MUST NOT require Bash, `curl`, `jq`, `yq`, Python, Rust, the ESDiag binary, a container runtime, or a Unix compatibility layer to collect diagnostics.

#### Scenario: Collect on a standard Windows host
- **GIVEN** a Windows host with Windows PowerShell 5.1 or newer
- **AND** valid Elasticsearch environment configuration
- **WHEN** the operator runs `powershell -File bin/esdiag-lite.ps1 collect --archive=none`
- **THEN** the script collects a processable Elasticsearch API diagnostic directory without invoking external Unix utilities

### Requirement: Equivalent Collection Configuration and Commands
The PowerShell script SHALL provide the `collect`, `watch`, `send`, and help commands; support `--archive=zip|none` and `--send=<id>`; and use `ELASTIC_ES_URL`, `ELASTIC_ES_API_KEY`, `ELASTIC_ES_USERNAME`, and `ELASTIC_ES_PASSWORD` with the same API-key precedence rules as `esdiag-lite.sh`. It SHALL support `UPLOAD_HOST`, `UPLOAD_ID`, and `send <filename> [id]` with the same explicit-Send semantics as the Bash collector.

#### Scenario: API key takes precedence on Windows
- **GIVEN** `ELASTIC_ES_URL`, `ELASTIC_ES_API_KEY`, `ELASTIC_ES_USERNAME`, and `ELASTIC_ES_PASSWORD` are non-empty
- **WHEN** the PowerShell collector sends an Elasticsearch request
- **THEN** it uses API-key authentication
- **AND** it does not use the username or password

#### Scenario: Send an existing archive
- **GIVEN** `UPLOAD_ID` is non-empty
- **AND** an existing ZIP diagnostic archive is supplied as the filename argument
- **WHEN** the operator runs `send <filename>`
- **THEN** the script sends that archive using the environment Elastic Upload Service ID

### Requirement: Generated Version-Aware Lite APIs
The repository generator SHALL render named PowerShell API functions and the ordered collection sequence from the `lite`-tagged Elasticsearch source definitions. The rendered functions SHALL select exactly one API endpoint for a detected supported version, delegate HTTP execution to shared handwritten PowerShell request code, and skip unsupported APIs without failing the remaining collection.

#### Scenario: Generate a version-dependent PowerShell API
- **GIVEN** a `lite` source has request paths on either side of an Elasticsearch version boundary
- **WHEN** the generator renders `bin/esdiag-lite.ps1`
- **THEN** the source has a named PowerShell function with numeric version predicates for each supported path
- **AND** the script requests only the path appropriate to the detected cluster version

#### Scenario: Detect generator drift
- **GIVEN** a `lite` source definition changes
- **AND** `bin/esdiag-lite.ps1` has not been regenerated
- **WHEN** the repository generation check runs
- **THEN** it fails and identifies the PowerShell artifact as stale

### Requirement: Processable Archive and Send Output
The PowerShell collector SHALL create the same `api-diagnostics-<timestamp>` directory and `diagnostic_manifest.json` layout as the Bash collector. ZIP mode SHALL create `api-diagnostics-<timestamp>.zip` with files at the archive root; none mode SHALL retain the directory. An explicitly requested Send SHALL transfer the completed ZIP archive to Elastic Upload Service using SHA-256 digests, 50,000,000-byte parts, resumable existing-part checks, and finalization.

#### Scenario: Archive a successful PowerShell collection
- **GIVEN** ZIP output is selected and PowerShell ZIP support is available
- **WHEN** collection succeeds
- **THEN** the script creates a root-layout ZIP archive
- **AND** removes the source directory only after archive creation succeeds

#### Scenario: ZIP support is unavailable
- **GIVEN** ZIP output is selected
- **AND** the required PowerShell ZIP capability is unavailable
- **WHEN** dependencies are validated
- **THEN** the script exits before collection with actionable instructions to run with `--archive=none`

#### Scenario: Hand PowerShell output to ESDiag
- **GIVEN** the PowerShell script produced a ZIP archive or diagnostic directory
- **WHEN** an operator supplies it to `esdiag process`
- **THEN** ESDiag accepts it as an Elasticsearch diagnostic input

### Requirement: PowerShell Quality Gates and Documentation
The PowerShell artifact and generated region SHALL have automated syntax, static-analysis, formatting, and behavior validation appropriate to the repository's supported PowerShell tooling. Maintained documentation SHALL label `esdiag-lite.ps1` as a collection-only utility, document Windows invocation and prerequisites, and direct operators to ESDiag for diagnostic processing.

#### Scenario: Validate the PowerShell artifact
- **GIVEN** `bin/esdiag-lite.ps1` has been generated
- **WHEN** repository validation runs the configured PowerShell quality gates
- **THEN** syntax, static analysis, formatting, and relevant behavior tests complete without findings
