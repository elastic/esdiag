## ADDED Requirements

### Requirement: Collect Command Optional Upload Handoff
The system SHALL allow `esdiag collect` to accept an optional `--upload` argument containing an Elastic Upload Service upload identifier or URL. When this argument is present, the collect command SHALL perform its normal collection behavior first and then begin an upload step for the archive it just produced. The existing `-u` shorthand SHALL remain reserved for the collect command's `--user` metadata option.

#### Scenario: Collect succeeds with upload handoff enabled
- **GIVEN** the user provides a valid collect host, a valid local output location, and a valid Elastic Upload Service upload identifier
- **WHEN** the user runs `esdiag collect <host> <output> --upload <upload_id>`
- **THEN** the system completes the collect step and writes a local diagnostic archive
- **AND** the system begins an upload step for that collected archive using the provided `upload_id`

#### Scenario: Collect without upload flag remains unchanged
- **GIVEN** the user provides a valid collect host and a valid local output location
- **WHEN** the user runs `esdiag collect <host> <output>` without `--upload`
- **THEN** the system completes the collect step and writes a local diagnostic archive
- **AND** the system does not invoke the Elastic Upload Service uploader

### Requirement: Collect Upload Handoff Uses Resolved Archive Path
The collect upload handoff SHALL use the actual archive path produced by the collect step, including a runtime-generated filename when the final archive name is not known in advance.

#### Scenario: Collect generates the archive filename at runtime
- **GIVEN** the collect workflow determines the final archive filename during execution
- **WHEN** the user runs `esdiag collect <host> <output> --upload <upload_id>`
- **THEN** the system resolves the final emitted archive path from the completed collect step
- **AND** the upload handoff uses that resolved archive path instead of requiring the user to supply the generated filename

#### Scenario: Collect fails before producing an archive
- **GIVEN** the user runs `esdiag collect <host> <output> --upload <upload_id>`
- **WHEN** the collect step fails before producing a diagnostic archive
- **THEN** the command returns the collect failure
- **AND** the system does not attempt the upload handoff
