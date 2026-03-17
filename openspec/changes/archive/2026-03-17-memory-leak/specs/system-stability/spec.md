## ADDED Requirements

### Requirement: Memory Management

The application MUST properly reclaim all resources associated with a processing job once the job completes successfully or fails.

#### Scenario: Sequential job execution
- **GIVEN** the esdiag server is running
- **WHEN** multiple diagnostic archives are uploaded and processed sequentially
- **THEN** the resident memory usage of the process MUST NOT continuously increase over time.

#### Scenario: Concurrent job execution
- **GIVEN** the esdiag server is running
- **WHEN** multiple diagnostic archives are uploaded and processed concurrently
- **THEN** the resident memory usage of the process MUST NOT continuously increase over time and must stabilize after jobs complete.