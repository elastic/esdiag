## Why

Diagnostic bundles from orchestration platforms like Elastic Cloud Kubernetes (ECK) or Elastic Cloud Enterprise (ECE) may contain nested or related diagnostics. Currently, these relationships are not explicitly tracked in the metadata, making it difficult to filter related diagnostics or identify the hosting orchestration layer during analysis.

## What Changes

- **Metadata Enrichment**: Automatically populate `diagnostic.parent_id` when `included_diagnostics` is detected in `diagnostic_manifest.json`.
- **Orchestration Identification**: Introduce a `diagnostic.orchestration` field to identify the hosting platform. Supported platforms include:
    - Elastic Cloud Kubernetes
    - Elastic Cloud Enterprise
    - Elastic Cloud Hosted
    - Kubernetes Platform

## Capabilities

### New Capabilities
- `orchestration-metadata`: Requirements for identifying and recording relationship and platform metadata from orchestration-provided diagnostic manifests.

### Modified Capabilities
(None)

## Impact

- **Manifest Parsing**: Updates to `diagnostic_manifest.json` parsing logic to extract parent/child relationships.
- **Metadata Schema**: Addition of new fields to the core diagnostic metadata model.
- **Downstream Tools**: Improved filtering and grouping for tools consuming the diagnostic metadata.
