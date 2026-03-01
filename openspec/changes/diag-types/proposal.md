## Why

Currently, the `collect` command in ESDiag collects a fixed set of diagnostic APIs without allowing metadata enrichment at collection time, and does so purely sequentially without robust retry mechanisms. Users need the ability to select which APIs to collect to tailor diagnostic bundles for specific scenarios (e.g., support, minimal, standard) and to include or exclude specific APIs based on their troubleshooting needs, while automatically resolving required dependencies. Furthermore, users currently enrich diagnostics with metadata (e.g., account, case, opportunity, user) during the `process` phase, but cannot record this information natively at the time of `collect`. Finally, we must ensure high-quality output and cluster stability by implementing graceful retries on failures, deduplicating requested APIs, and distinguishing between heavy (sequential) and lightweight (concurrent) API collections.

## What Changes

- Add CLI arguments for selecting predefined diagnostic types (`--type support`, `--type minimal`, `--type standard`, `--type comprehensive`).
- Add CLI arguments for explicitly including or excluding specific APIs (`--include`, `--exclude`).
- Implement an API dependency resolution mechanism to ensure required endpoints (e.g., `nodes_settings` for `nodes_stats`) are automatically included, with strict deduplication.
- Establish a baseline of minimum required endpoints for any diagnostic collection type.
- Add CLI arguments to `collect` for diagnostic Identifiers (`--account`, `--case`, `--opportunity`, `--user`), matching the arguments available on `process`.
- Record these identifiers directly in the generated `DiagnosticManifest` under a new `identifiers` property.
- Implement an API classification system (`Heavy` vs `Light`) to allow concurrent collection of lightweight APIs while enforcing sequential collection for heavy APIs to protect cluster stability.
- Implement a graceful retry mechanism for transient API collection failures, logging warnings on failure.
- Ensure the collection execution loop uses exhaustive matching over the defined APIs to prevent compile-time blind spots.

## Capabilities

### New Capabilities
- `api-selection`: Defines the mechanism for selecting, filtering, and resolving dependencies for diagnostic API collection.
- `collection-identifiers`: Defines the ability to capture and store metadata identifiers (account, case, etc.) at collection time within the diagnostic manifest.
- `collection-execution`: Defines the rules for API deduplication, concurrency grouping (Heavy/Light), graceful retries, and exhaustive pattern matching during execution.

### Modified Capabilities

## Impact

- **CLI**: The `collect` command will have new arguments for `--type`, `--include`, `--exclude`, `--account`, `--case`, `--opportunity`, and `--user`.
- **Core Processing Logic**: The collector orchestrator will dynamically construct and deduplicate the list of APIs to fetch. The execution loop will be split into a concurrent phase (Light APIs) and a sequential phase (Heavy APIs), wrapped in retry logic.
- **Dependencies**: Resolving API dependencies will introduce a pre-collection validation and dependency resolution step.
- **Manifest Format**: The `manifest.json` will now include an `identifiers` object and a `collected_apis` array.
