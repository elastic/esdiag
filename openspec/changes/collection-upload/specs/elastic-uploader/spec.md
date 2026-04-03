## ADDED Requirements

### Requirement: Collect Command Reuses Elastic Uploader
When `esdiag collect` is invoked with `--upload`, the system SHALL reuse the Elastic Upload Service uploader capability to upload the collected raw diagnostic bundle after collection succeeds.

#### Scenario: Collect hands off a raw bundle to the uploader
- **GIVEN** a collect run has completed successfully and produced a local diagnostic archive
- **AND** the user provided `--upload <upload_id>` on the collect command
- **WHEN** the upload handoff begins
- **THEN** the system invokes the Elastic Upload Service uploader capability for the collected archive
- **AND** the uploader sends the raw archive bytes unchanged

### Requirement: Collect Upload Failure Surfaces After Successful Collection
If the collect step succeeds and the upload handoff fails, the system MUST report the upload failure from the collect command while preserving the already collected local archive.

#### Scenario: Upload fails after archive collection succeeds
- **GIVEN** the collect step has already written a local diagnostic archive successfully
- **AND** the user provided `--upload <upload_id>` on the collect command
- **WHEN** the Elastic Upload Service uploader fails during upload validation, transfer, or finalize
- **THEN** the collect command returns an error for the failed upload step
- **AND** the previously collected local archive remains available for retry or inspection
