## ADDED Requirements

### Requirement: Kibana Metadata Extraction
The system SHALL extract common metadata from the diagnostic bundle to enrich all exported Kibana documents.

#### Scenario: Successful metadata extraction
- **WHEN** a Kibana diagnostic is initialized
- **THEN** every exported document contains `diagnostic.id`, the collector version in `diagnostic.version`, top-level `@timestamp`, and `node.name`, `node.id`, and `node.version.number` as shared processing context
