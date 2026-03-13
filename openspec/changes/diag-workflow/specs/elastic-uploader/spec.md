## ADDED Requirements

### Requirement: Upload Command For Raw Diagnostic Bundles
The system SHALL provide a CLI command `esdiag upload <file_name> <upload_id>` for sending an unprocessed diagnostic bundle to Elastic Upload Service.

#### Scenario: User uploads a diagnostic bundle from CLI
- **GIVEN** a local diagnostic archive file and an Elastic Upload Service upload identifier
- **WHEN** the user runs `esdiag upload <file_name> <upload_id>`
- **THEN** the system uploads the unprocessed diagnostic bundle to Elastic Upload Service

### Requirement: Workflow Uses Elastic Uploader For Forwarded Remote Send
When the workflow is configured for `Process -> Forward` and `Send -> Remote`, the system SHALL use the Elastic Upload Service uploader capability instead of the processed-diagnostic exporter path.

#### Scenario: Forwarded archive uses uploader capability
- **GIVEN** the workflow is configured to forward a raw archive remotely
- **WHEN** the user executes the send step
- **THEN** the system invokes the Elastic Upload Service uploader path for the archive
- **AND** it does not invoke processed-document export behavior

### Requirement: Upload Command Preserves Raw Archive
The uploader capability SHALL send the raw diagnostic bundle unchanged. It SHALL NOT attempt to process the archive into diagnostic documents before upload.

#### Scenario: Raw archive remains unprocessed during upload
- **GIVEN** a diagnostic archive selected for uploader delivery
- **WHEN** the upload command or workflow uploader path runs
- **THEN** the archive bytes are uploaded as-is
- **AND** no processor pipeline is executed before upload
