## Context

Currently, the `ElasticsearchCollector` (and similar collectors like Logstash) executes a hard-coded, sequential list of API calls to gather diagnostic information. As ESDiag scales, we need to provide users the ability to perform targeted collections. For instance, a "minimal" run might only collect cluster state and node information, whereas a "support" run might collect a broad set of APIs. Additionally, users may want to explicitly `--include` or `--exclude` certain APIs.

Furthermore, `esdiag process` allows users to specify identifiers (`--account`, `--case`, `--opportunity`, `--user`) to enrich the processed diagnostic data. Currently, there is no way to record these identifiers natively at the time of collection (`esdiag collect`). Finally, we must ensure that a dynamic API collection mechanism does not compromise cluster safety (via heavy concurrent requests) or output quality (via duplicate collections or missing retry logic). 

This design addresses:
1. The implementation of dynamic API selection based on predefined types (support, minimal, standard, comprehensive) and explicit user overrides.
2. Enabling the collection phase to capture metadata identifiers (account, case, etc.) and record them directly in the generated `DiagnosticManifest`.
3. An execution strategy ensuring API deduplication, robust retry logic, and safety-aware concurrency (distinguishing Heavy vs Light APIs).

This change implements issue: https://github.com/elastic/esdiag/issues/123

## Goals / Non-Goals

**Goals:**
- Implement CLI arguments `--type`, `--include`, and `--exclude` for the `collect` command.
- Implement CLI arguments `--account`, `--case`, `--opportunity`, and `--user` for the `collect` command.
- Define a structured registry using Enum dispatch for mapping API identifiers to their actual collection routines. Ensure exhaustive pattern matching for safety.
- Implement common dependency resolution logic that strictly deduplicates API entries.
- Fail fast if a requested API identifier is invalid for the target product.
- Categorize APIs internally as `Heavy` or `Light`. Execute `Light` APIs concurrently for speed, but `Heavy` APIs sequentially to protect the target cluster.
- Implement graceful retries for failed API fetches to ensure high-quality output, logging warnings on failures.
- Record the final resolved list of APIs and the provided Identifiers in the `DiagnosticManifest`.

**Non-Goals:**
- Refactoring how identifiers are applied during `process` beyond what's needed to read them from the manifest.

## Decisions

**1. API Identifiers and Exhaustive Enum Dispatch**
We will introduce a registry mechanism within the collector using Enum dispatch. We will define a product-specific enum (e.g., `ElasticsearchApi`) and implement an exhaustive `match` statement (no `_` catch-all) to route string identifiers to the corresponding `self.save::<T>()` logic. This ensures compile-time safety when new APIs are added to the enum.

**2. Product Validation and Deduplication**
The orchestrator will maintain a static list of valid API identifiers per product. During the dependency resolution phase:
- User inputs (`--include`, `--exclude`) are validated against this list. Invalid identifiers cause an immediate failure.
- The resolution logic will use an `IndexSet` (or similar ordered set) to strictly prevent duplicate entries if an API is requested explicitly and also automatically resolved as a dependency.

**3. Execution Strategy: Heavy vs Light APIs**
We will add an internal property/trait `Weight` (`Light`, `Heavy`) to the registered APIs. 
- `Heavy` APIs (e.g., `nodes_stats`, `mapping`) MUST be collected strictly sequentially to avoid CPU spikes on the Elasticsearch cluster.
- `Light` APIs (e.g., `cluster_health`, `licenses`) can be safely collected concurrently to speed up the collection phase.

**4. Graceful Retries**
The execution loop surrounding `self.save::<T>()` will be wrapped in a retry mechanism using an exponential backoff timer. It will retry transient HTTP fetch failures for up to 5 minutes. If a fetch fails, we will gracefully retry, log a warning, and if ultimately failed after the 5-minute window, skip the API rather than crashing the entire diagnostic collection run.

**5. Dependency Graph and Resolution (Common Logic)**
A common API resolver will orchestrate this:
1. Start with the base set of API identifiers from the selected `--type`.
2. Apply any `--include` additions.
3. Apply any `--exclude` removals.
4. Add the minimum required APIs.
5. Recursively resolve dependencies and insert them into the unique set.

**6. CLI Arguments**
In the `clap` struct for the `collect` command, we will add:
- API Selectors: `--type`, `--include`, `--exclude`.
- Identifiers: `--account` (`-a`), `--case` (`-c`), `--opportunity` (`-o`), `--user` (`-u`).

**7. Manifest Enrichment**
The `DiagnosticManifest` struct will be updated to include two new properties:
- `collected_apis`: A unique list of strings documenting the exact, final resolved APIs targeted.
- `identifiers`: An `Identifiers` object representing the metadata provided by the user.

## Risks / Trade-offs

- **Risk:** Implementing concurrency for `Light` APIs might accidentally overwhelm small clusters if there are too many concurrent connections.
  - *Mitigation:* The concurrency factor should be bounded (e.g., using `futures::stream::StreamExt::buffer_unordered` with a limit like 5).
- **Trade-off:** Expanding the `DiagnosticManifest` means older versions of `esdiag process` might ignore these fields (which is fine, due to serde's forgiving nature), while newer versions can read `identifiers` from the manifest if they aren't provided via CLI at processing time.
