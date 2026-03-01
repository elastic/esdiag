## ADDED Requirements

### Requirement: CLI Arguments for Identifiers during Collection
The system SHALL provide CLI arguments for the `collect` command to capture metadata identifiers: `--account` (`-a`), `--case` (`-c`), `--opportunity` (`-o`), and `--user` (`-u`). These MUST mirror the identifier arguments currently available in the `process` command.

#### Scenario: User provides identifiers during collect
- **GIVEN** a collector orchestrator is invoked
- **WHEN** the user runs `esdiag collect --account "Acme" --case "12345" --user "Jane"`
- **THEN** the system captures the provided identifiers in an `Identifiers` object

### Requirement: Recording Identifiers in Diagnostic Manifest
The system SHALL store the provided metadata identifiers within the `DiagnosticManifest` object (e.g., `manifest.json`) generated at the time of collection. 

#### Scenario: Manifest serialization
- **GIVEN** the user provided identifiers during collection
- **WHEN** the collector successfully completes and serializes the `DiagnosticManifest`
- **THEN** the manifest file contains a new `identifiers` property
- **AND** the `identifiers` object includes the values provided via CLI (e.g., `"account": "Acme", "case": "12345"`)
