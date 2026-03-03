## ADDED Requirements

### Requirement: Collect supports zip output mode
The system SHALL expose a `--zip` option on the `collect` command that enables writing the diagnostic as a single zip archive.

#### Scenario: Zip mode enabled for collect
- **WHEN** a user runs `esdiag collect --zip`
- **THEN** the command writes one `.zip` artifact for the diagnostic output
- **AND** it does not require the user to run a separate bundling step

### Requirement: Collect zip destination semantics
The `collect --zip` option SHALL be a boolean mode switch, and archive destination SHALL be controlled by the `collect` command's `output` positional argument (defaulting to current directory `.`).

#### Scenario: Collect zip with default destination
- **WHEN** a user runs `esdiag collect --zip`
- **THEN** the archive is written to the current working directory

#### Scenario: Collect zip with explicit destination directory
- **WHEN** a user runs `esdiag collect <host> /tmp/out --zip`
- **THEN** the archive is written under `/tmp/out`

#### Scenario: Collect zip rejects non-directory destination
- **WHEN** a user runs `esdiag collect <host> /tmp/out.file --zip` and `/tmp/out.file` already exists as a file
- **THEN** the command fails with a destination validation error indicating a directory is required

### Requirement: Collect directory destination safety
The `collect` command without `--zip` SHALL reject output paths that already exist as regular files.

#### Scenario: Collect non-zip rejects existing file destination
- **WHEN** a user runs `esdiag collect <host> /tmp/out.file` and `/tmp/out.file` already exists as a file
- **THEN** the command fails with a destination validation error indicating a directory is required

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
The system SHALL expose a `--zip` option on the `process` command that stores collected API outputs in a single archive using the standard diagnostic naming format (for example, `api-diagnostics-<timestamp>.zip`).

### Requirement: Process zip destination semantics
The `process --zip` option SHALL accept an optional path interpreted as an output directory, where the default destination is the current directory (`.`) when no explicit path is provided.

#### Scenario: Process emits diagnostic zip artifact
- **WHEN** a user runs `esdiag process --zip`
- **THEN** all API output files for that diagnostic are written to one `api-diagnostics-*.zip` archive

#### Scenario: Process zip with explicit destination directory
- **WHEN** a user runs `esdiag process --zip /tmp/out`
- **THEN** the archive is written under `/tmp/out`

#### Scenario: Process zip rejects non-directory destination
- **WHEN** a user runs `esdiag process --zip /tmp/out.file` and `/tmp/out.file` already exists as a file
- **THEN** the command fails with a destination validation error indicating a directory is required
