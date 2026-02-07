## Why

The tasks processor in `esdiag` currently uses `.expect()` when retrieving node metadata for a task. In certain environments, such as Elasticsearch Serverless, node metadata might be missing or structured differently, leading to a panic that terminates the diagnostic processing prematurely.

## What Changes

- Modified the task enrichment logic to gracefully handle cases where node metadata is missing.
- Replaced `.expect("Node not found for task")` with safe `Option` handling.
- Added error logging when node metadata is missing, while allowing the processing of the task document to continue.

## Capabilities

### New Capabilities
- None

### Modified Capabilities
- `diagnostic-reporting`: Strengthened the robustness of the processing pipeline to handle missing metadata without panicking.

## Impact

- `src/processor/elasticsearch/tasks/processor.rs`: Updated the `documents_export` method and `EnrichedTask` struct to support optional node metadata.
- Processing logic: Increased resilience to environment-specific metadata variations.
