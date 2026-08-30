## Purpose

Define the stable structured-output contract for finite, streaming, and long-running `esdiag` CLI commands.

## Requirements

### Requirement: Finite CLI Commands Emit Typed YAML Results
Every finite `esdiag` command execution SHALL emit exactly one typed result document to stdout after successful completion. The default representation SHALL be pretty-printed block-style YAML, SHALL contain a stable snake_case `result` discriminator, and MUST NOT require terminal detection or agent mode to activate.

#### Scenario: Process emits a YAML diagnostic result
- **GIVEN** a diagnostic processes successfully to a non-stream output
- **WHEN** the user runs `esdiag process <input> <output>` without selecting a format
- **THEN** stdout contains one valid YAML document
- **AND** its `result` identifies a processed diagnostic
- **AND** it contains the primary `diagnostic.id`, created document count, integer duration in milliseconds, and Kibana URL when available

#### Scenario: Mutation emits a typed result
- **WHEN** the user successfully adds, updates, or removes a host, secret, or saved job
- **THEN** stdout contains one YAML result identifying the performed mutation and affected resource
- **AND** it does not contain a prose-only completion message

#### Scenario: List emits a typed collection
- **WHEN** the user runs `esdiag host list` or `esdiag job list`
- **THEN** stdout contains one YAML result with a sequence of typed entries
- **AND** an empty persisted collection is represented by an empty sequence rather than a table or empty-state sentence

### Requirement: All Finite Command Families Have Explicit Outcomes
The CLI SHALL define explicit compact outcomes for successful collect, process, send, setup, host, keystore, and saved-job command families. Each outcome MUST expose the durable facts a caller needs to identify what changed or what artifact was produced, and MUST NOT serialize an entire internal processor state or diagnostic report.

#### Scenario: Collect outcome identifies its artifact
- **WHEN** collection completes successfully
- **THEN** the result contains the resolved archive or diagnostic path and successful and total file counts
- **AND** it contains send destination metadata when the invocation also sent the archive

#### Scenario: Job run preserves every completed stage result
- **WHEN** `esdiag job run <name>` completes
- **THEN** the result identifies the completed job
- **AND** includes a save result when the job retained a newly collected archive
- **AND** includes a process result when the job processed a diagnostic
- **AND** includes a send result when the job sent a bundle
- **AND** may contain all three results when the selected stages produced them in one run

#### Scenario: Keystore status exposes facts without prose parsing
- **WHEN** the user runs `esdiag keystore status`
- **THEN** the result exposes whether the keystore exists, whether the unlock lease is active, and its expiration when present
- **AND** callers do not need to parse a colored status sentence

### Requirement: CLI Output Format Is Explicit and Stable
The CLI SHALL provide a global `--format` option accepting `yaml` and `json`, with `yaml` as the default. YAML SHALL use readable block formatting; JSON SHALL be one compact JSON value. Result discriminators, field names, and enum values SHALL use snake_case and SHALL be versioned as part of the CLI compatibility contract.

#### Scenario: JSON explicitly requested
- **WHEN** a user runs a finite command with `--format json`
- **THEN** stdout contains one valid compact JSON value representing the same typed outcome available as YAML
- **AND** stderr logging behavior is unchanged

#### Scenario: Invalid format rejected by argument parser
- **WHEN** a user supplies an unsupported `--format` value
- **THEN** Clap rejects the invocation before command execution
- **AND** no command result is emitted

#### Scenario: Optional values remain concise
- **GIVEN** an outcome has no Kibana URL and no included diagnostics
- **WHEN** it is serialized
- **THEN** unneeded optional fields and empty collections are omitted where their absence is unambiguous
- **AND** required discriminator and identity fields remain present

### Requirement: Standard Output And Logs Have Separate Contracts
For finite commands, stdout SHALL contain only the structured terminal outcome. Operational tracing, warnings, debug detail, and progress SHALL remain on stderr and MUST NOT be embedded in the YAML or JSON result.

#### Scenario: Informational logs do not contaminate YAML
- **WHEN** a finite command runs at info or debug log level and succeeds
- **THEN** its stdout parses as exactly one YAML or JSON value
- **AND** operational log records appear only on stderr

#### Scenario: Agent mode retains the same schema
- **WHEN** a finite command runs with `--agent` or with `CLAUDECODE` present
- **THEN** it emits the same result schema and default YAML format as a normal invocation
- **AND** agent mode changes only the default stderr log level

### Requirement: Finite Command Failures Are Structured
After a finite command begins execution, a terminal failure SHALL emit one structured failure value to stdout in the selected format and exit non-zero. The failure SHALL contain a stable category and safe human-readable message, MAY include allowlisted command context, and MUST NOT expose credentials or an unrestricted internal error chain. HTTP response failures SHALL additionally include the response `status` and, when present, the response error `type` and `reason` fields verbatim to make troubleshooting possible without recovering logs. When a phase-composed job created durable results before a later stage failed, the failure SHALL retain those allowlisted completed-stage facts, identify the failed stage, and communicate whole-job retry safety without claiming overall success.

#### Scenario: HTTP failure exposes diagnostic response fields
- **GIVEN** a finite command receives an HTTP error response with `status`, `error.type`, and `error.reason`
- **WHEN** the command emits its structured failure
- **THEN** the result retains its stable CLI `category`
- **AND** it includes the response `status`, `type`, and `reason` values
- **AND** it exits non-zero

#### Scenario: Unknown saved job returns structured failure
- **WHEN** the user runs `esdiag job run unknown-name`
- **THEN** the process exits non-zero
- **AND** stdout contains one structured failure identifying a not-found category and the missing job name
- **AND** detailed diagnostics, when enabled, remain on stderr

#### Scenario: Successful result is not emitted after failure
- **WHEN** processing fails after command execution begins
- **THEN** stdout contains a failure value rather than a success outcome
- **AND** no fabricated diagnostic identifier or Kibana URL is present

#### Scenario: Job failure retains real earlier results
- **GIVEN** a finite saved job retained an archive and processed a diagnostic
- **WHEN** its later `Send` stage fails
- **THEN** stdout contains one non-zero failure identifying `send` as the failed stage
- **AND** includes the real retained archive and diagnostic facts under completed-stage context
- **AND** does not emit a `job_completed` result

#### Scenario: Usage output remains human-oriented
- **WHEN** Clap handles `--help`, `--version`, or an argument-parse failure before command execution
- **THEN** its normal human-readable usage behavior is preserved
- **AND** the CLI does not claim that usage text is a command outcome

### Requirement: Structured Streams Remain Single-Purpose
When a command intentionally assigns stdout to processed-document streaming, stdout SHALL retain only the existing NDJSON document schema. The CLI MUST NOT append a YAML, JSON, or prose completion result to that stream, regardless of the selected default format.

#### Scenario: Process streams NDJSON to stdout
- **WHEN** the user runs `esdiag process <input> -`
- **THEN** every stdout record remains an NDJSON processed-document record
- **AND** no terminal outcome of a different schema is appended

#### Scenario: Streaming command fails after partial output
- **GIVEN** a command has already written one or more NDJSON records to stdout
- **WHEN** the command fails
- **THEN** it exits non-zero without injecting a structured failure value into the NDJSON stream
- **AND** the operational error is reported on stderr

#### Scenario: Saved job exports processed documents to stdout
- **GIVEN** a saved job's `Process` stage uses the stdout export target
- **WHEN** the user runs `esdiag job run <name>`
- **THEN** every stdout record remains an NDJSON processed-document record
- **AND** no job-completion YAML, JSON, or prose value is appended

### Requirement: Long-Running Server Emits Structured Readiness
When stdout is not assigned to processed-document streaming, the `serve` command SHALL emit one structured readiness result after successfully binding its listener. The result SHALL identify the bound address, port, runtime mode, and configured output category without exposing credentials. It MUST NOT emit a second terminal result on ordinary shutdown.

#### Scenario: Server reports readiness
- **WHEN** `esdiag serve` binds successfully
- **THEN** stdout emits one YAML readiness result before waiting for shutdown
- **AND** automation can discover the actual bound address from typed fields

#### Scenario: Server exporter owns stdout
- **GIVEN** `serve` is configured with a processed-document stdout exporter
- **WHEN** the server starts
- **THEN** no readiness value contaminates the exporter stream
- **AND** readiness is reported as an operational stderr event for that mode

### Requirement: Structured Outcomes Exclude Secrets
CLI outcome types SHALL be allowlisted projections and MUST NOT serialize API keys, passwords, decrypted keystore material, authorization headers, or complete credential-bearing host records.

#### Scenario: Host list omits credential values
- **WHEN** `esdiag host list` serializes saved hosts
- **THEN** it may include a secret reference identifier
- **AND** it does not include the referenced secret value, API key, or password

#### Scenario: Failure redacts sensitive input
- **GIVEN** a command fails while handling credentials
- **WHEN** the structured failure is serialized
- **THEN** it contains a safe category and message
- **AND** it does not contain supplied credential material from the CLI, keystore, or error chain
- **AND** the explicitly supported HTTP response `reason` is preserved when the server includes it
