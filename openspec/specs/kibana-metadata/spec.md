# kibana-metadata Specification

## Purpose
Extract common diagnostic and node metadata from a Kibana diagnostic bundle and use it to enrich every exported Kibana document.

## Requirements

### Requirement: Kibana Metadata Extraction
The system SHALL extract common metadata from the diagnostic bundle to enrich all exported Kibana documents.

#### Scenario: Successful metadata extraction
- **GIVEN** a Kibana diagnostic bundle with a manifest and node status
- **WHEN** the Kibana diagnostic is initialized
- **THEN** every exported document contains `diagnostic.id`, the collector version in `diagnostic.version`, `diagnostic.platform`, `diagnostic.application`, top-level `@timestamp`, and `node.name`, `node.id`, and `node.version.number` as shared processing context

#### Scenario: Space-scoped metadata
- **GIVEN** a Kibana diagnostic bundle with files under `spaces/<space>/`
- **WHEN** a document is exported from one of those files
- **THEN** the document contains the space identifier in `space.id`, and documents from root or unscoped paginated files omit `space`
