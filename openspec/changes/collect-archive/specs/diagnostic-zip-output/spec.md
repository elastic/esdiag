## ADDED Requirements

### Requirement: Collect supports zip output mode
The system SHALL expose a `--zip` option on the `collect` command that enables writing the diagnostic as a single zip archive.

#### Scenario: Zip mode enabled for collect
- **WHEN** a user runs `esdiag collect --zip`
- **THEN** the command writes one `.zip` artifact for the diagnostic output
- **AND** it does not require the user to run a separate bundling step

### Requirement: Collect zip destination semantics
The `collect --zip` option SHALL accept an optional path interpreted as an output directory, where the default destination is the current directory (`.`) when no explicit path is provided.

#### Scenario: Collect zip with default destination
- **WHEN** a user runs `esdiag collect --zip` without a path
- **THEN** the archive is written to the current working directory

#### Scenario: Collect zip with explicit destination directory
- **WHEN** a user runs `esdiag collect --zip /tmp/out`
- **THEN** the archive is written under `/tmp/out`

### Requirement: Collect zip filename parity
When `collect --zip` is enabled, the output archive filename SHALL reuse the existing directory-output diagnostic naming format and append `.zip`.

#### Scenario: Filename is derived from existing diagnostic name
- **WHEN** the diagnostic base name resolves to `diagnostic-abc-2026-Mar-02--09_15_30`
- **THEN** the zip filename is `diagnostic-abc-2026-Mar-02--09_15_30.zip`

### Requirement: Collect writes directly to archive
In zip mode, the system MUST write API output entries directly into the target zip file as data is produced and MUST NOT first materialize a full directory tree for later bundling.

#### Scenario: Direct archive write path
- **WHEN** API responses are fetched during `collect --zip`
- **THEN** each response is written into an archive entry in the target zip file
- **AND** no final "bundle directory into zip" pass is required

### Requirement: Process supports zip output mode
The system SHALL expose a `--zip` option on the `process` command that stores all processed API outputs in a `{diagnostic}.zip` archive.

#### Scenario: Process emits diagnostic zip artifact
- **WHEN** a user runs `esdiag process --zip`
- **THEN** all API output files for that diagnostic are written to `{diagnostic}.zip`
