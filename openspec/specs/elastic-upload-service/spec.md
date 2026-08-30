# elastic-upload-service

## Purpose

Defines the Elastic Upload Service adapter for sending raw diagnostic bundles,
including CLI entry points and workflow integration.

## ADDED Requirements

### Requirement: Send Command For Raw Diagnostic Bundles
The system SHALL provide a CLI command `esdiag send <file_name> <upload_id>` for sending an unprocessed diagnostic bundle to Elastic Upload Service.

#### Scenario: User sends a diagnostic bundle from CLI
- **GIVEN** a local diagnostic archive file and an Elastic Upload Service identifier
- **WHEN** the user runs `esdiag send <file_name> <upload_id>`
- **THEN** the system sends the unprocessed diagnostic bundle to Elastic Upload Service

### Requirement: Workflow Uses Elastic Upload Service For Forwarded Remote Send
When the workflow is configured for `Process -> Forward` and `Send -> Remote`, the system SHALL use the Elastic Upload Service adapter instead of the processed-diagnostic exporter path.

#### Scenario: Forwarded archive uses Elastic Upload Service adapter
- **GIVEN** the workflow is configured to forward a raw archive remotely
- **WHEN** the user executes the send step
- **THEN** the system invokes the Elastic Upload Service adapter for the archive
- **AND** it does not invoke processed-document export behavior

### Requirement: Send Command Preserves Raw Archive
The Elastic Upload Service adapter SHALL send the raw diagnostic bundle unchanged. It SHALL NOT attempt to process the archive into diagnostic documents before sending.

#### Scenario: Raw archive remains unprocessed during send
- **GIVEN** a diagnostic archive selected for Elastic Upload Service delivery
- **WHEN** the send command or workflow Send stage runs
- **THEN** the archive bytes are sent as-is
- **AND** no processor pipeline is executed before sending

### Requirement: Collect Command Reuses Elastic Upload Service Adapter
When `esdiag collect` is invoked with `--send`, the system SHALL reuse the Elastic Upload Service adapter to send the collected raw diagnostic bundle after collection succeeds.

#### Scenario: Collect hands off a raw bundle to the Send stage
- **GIVEN** a collect run has completed successfully and produced a local diagnostic archive
- **AND** the user provided `--send <upload_id>` on the collect command
- **WHEN** the Send handoff begins
- **THEN** the system invokes the Elastic Upload Service adapter for the collected archive
- **AND** the adapter sends the raw archive bytes unchanged

### Requirement: Collect Send Failure Surfaces After Successful Collection
If the collect step succeeds and the Send handoff fails, the system MUST report the send failure from the collect command while preserving the already collected local archive.

#### Scenario: Send fails after archive collection succeeds
- **GIVEN** the collect step has already written a local diagnostic archive successfully
- **AND** the user provided `--send <upload_id>` on the collect command
- **WHEN** the Elastic Upload Service adapter fails during validation, transfer, or finalization
- **THEN** the collect command returns an error for the failed Send stage
- **AND** the previously collected local archive remains available for retry or inspection
