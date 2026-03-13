## ADDED Requirements

### Requirement: Remote Collection Bundle Persistence
The workflow SHALL support optionally saving a remotely collected diagnostic archive to a user-specified local directory before later workflow stages execute. This persistence behavior SHALL reuse the same archive-save logic used by the CLI `collect --save` path. If bundle saving is disabled, the workflow MAY continue with the in-memory or temporary collected artifact without creating a persisted local copy.

#### Scenario: Save a remotely collected bundle locally
- **GIVEN** the user starts a remote diagnostic collection and enables `Save Bundle`
- **WHEN** the collection completes successfully
- **THEN** the system writes the collected archive to the configured local directory
- **AND** subsequent processing or send steps consume the saved archive or its equivalent normalized workflow artifact

#### Scenario: Workflow save reuses collect save behavior
- **GIVEN** the workflow enables archive saving during remote collection
- **WHEN** the system persists the collected archive locally
- **THEN** it uses the same archive-save behavior as the CLI `collect --save` path
- **AND** the saved archive is suitable for later processing or forwarding

### Requirement: One-Job and Two-Job Workflow Modes
The workflow SHALL support both a single-job on-demand path and a two-job saved-artifact path. `Collect -> Collect -> Process -> Send` without save SHALL preserve the current on-demand API retrieval behavior as one job. When save is enabled, collection SHALL complete as one job and processing-plus-send SHALL run as a second job using the saved archive.

#### Scenario: Unsaved collect-process-send remains on-demand
- **GIVEN** the user selects remote collection followed by processing and send
- **AND** save is disabled
- **WHEN** the workflow executes
- **THEN** collection, processing, and send run as the current on-demand flow without creating an intermediate saved archive job boundary

#### Scenario: Saved collect-process-send becomes two jobs
- **GIVEN** the user selects remote collection followed by processing and send
- **AND** save is enabled
- **WHEN** the workflow executes
- **THEN** collection completes as its own job that persists an archive
- **AND** processing and send run as a second job consuming that saved archive

### Requirement: Collect-Without-Process Workflow
The workflow SHALL support sending a collected diagnostic without invoking processing when the `Process` stage is configured for forwarding.

#### Scenario: Collect and send without processing
- **GIVEN** the user has configured a valid collect source
- **AND** the `Process` stage is configured for forwarding
- **WHEN** the workflow runs through collection and send
- **THEN** the system completes the collect stage without creating processed diagnostic documents
- **AND** the send stage receives the collected archive as its input artifact
