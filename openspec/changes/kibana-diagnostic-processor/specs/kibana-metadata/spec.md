## ADDED Requirements

### Requirement: Kibana Metadata Extraction
The system SHALL extract common metadata from the diagnostic bundle to enrich all exported Kibana documents.

#### Scenario: Successful metadata extraction
- **WHEN** a Kibana diagnostic is initialized
- **THEN** every exported document contains `diagnostic.id`, the collector version in `diagnostic.version`, `diagnostic.platform`, `diagnostic.application`, top-level `@timestamp`, and `node.name`, `node.id`, and `node.version.number` as shared processing context

#### Scenario: Space-scoped metadata
- **WHEN** a document is exported from a file under `spaces/<space>/`
- **THEN** the document contains the space identifier in `space.id`, and documents from root or unscoped paginated files omit `space`
