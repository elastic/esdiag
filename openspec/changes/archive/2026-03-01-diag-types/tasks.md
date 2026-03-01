## 1. Setup CLI Arguments

- [x] 1.1 Add `--type` argument to `esdiag collect` command in Clap structure. Values: `minimal`, `standard`, `support`, `comprehensive`. Default: `standard`.
- [x] 1.2 Add `--include` argument to `esdiag collect` command in Clap structure (accepts comma-separated list of strings via `value_delimiter(',')`).
- [x] 1.3 Add `--exclude` argument to `esdiag collect` command in Clap structure (accepts comma-separated list of strings via `value_delimiter(',')`).
- [x] 1.4 Add `account`, `case`, `opportunity`, and `user` arguments to `esdiag collect` command in Clap structure, mirroring the ones available on the `process` command.

## 2. Define API Models and Enum Dispatch

- [x] 2.1 Create a `DiagnosticType` enum to represent the CLI type argument.
- [x] 2.2 Define an enum `ElasticsearchApi` and an enum `LogstashApi` to serve as the strongly typed, product-specific identifiers for Enum dispatch.
- [x] 2.3 Create mapping logic to convert string identifiers to `ElasticsearchApi` and `LogstashApi` enums (for validation).
- [x] 2.4 Create an `ApiWeight` struct/trait (`Heavy`, `Light`) and attach it to the `ElasticsearchApi` and `LogstashApi` variants.
- [x] 2.5 Create a static mapping or method returning the set of API identifiers for each `DiagnosticType` variant per product (where `standard` equals the legacy hardcoded list).
- [x] 2.6 Create a static mapping for API dependencies (e.g., `"nodes_stats" -> vec!["nodes"]`).
- [x] 2.7 Define a static list of minimum required APIs per product (e.g., `"cluster"`, `"diagnostic_manifest"` for ES).

## 3. Implement Common Dependency Resolution

- [x] 3.1 Implement a common resolver function that takes a product's valid API list, base set from `--type`, `--include`/`--exclude` overrides, required minimums, and dependencies.
- [x] 3.2 Implement fast-failure logic in the resolver to return an error immediately if an `--include` or `--exclude` API identifier is not in the product's valid API list.
- [x] 3.3 Ensure the resolver uses an `IndexSet` (or similar ordered set) to prevent duplicate API identifiers in the final resolved list.
- [x] 3.4 Write unit tests for the dependency resolution logic: acyclic resolution, required APIs cannot be excluded, invalid APIs fail fast, strictly deduplicated final sets, and dependencies correctly added.

## 4. Refactor Collector Orchestrators and Manifest

- [x] 4.1 Update the `DiagnosticManifest` struct in `src/processor/diagnostic/diagnostic_manifest.rs` to include an `identifiers: Option<Identifiers>` field.
- [x] 4.2 Update the `DiagnosticManifest` struct to include a `collected_apis: Option<Vec<String>>` field.
- [x] 4.3 Update `ElasticsearchCollector` and `LogstashCollector` to resolve their respective API lists before beginning collection.
- [x] 4.4 Update collector orchestration to capture and pass the new `Identifiers` objects.
- [x] 4.5 Ensure `DiagnosticManifest` generation uses the final resolved API list to populate `collected_apis` and includes the captured `identifiers`.

## 5. Implement Collection Execution Loop

- [x] 5.1 Implement a retry wrapper using an exponential backoff timer (retrying for up to 5 minutes) around the `self.save::<T>()` logic, logging warnings on failure.
- [x] 5.2 Implement an execution orchestrator that splits the resolved API list into `Light` APIs and `Heavy` APIs.
- [x] 5.3 Implement bounded concurrency (e.g., using `futures::stream::StreamExt::buffer_unordered`) for `Light` APIs.
- [x] 5.4 Implement strictly sequential execution for `Heavy` APIs.
- [x] 5.5 Replace the hardcoded sequence of `self.save::<T>()` calls in both collectors with an exhaustive Enum dispatch `match` statement mapping the resolved API enum values to their respective data sources. Ensure there is no `_` catch-all arm.

## 6. Verification

- [x] 6.1 Run `cargo test` to verify no regressions in existing code and that new tests pass.
- [x] 6.2 Run `cargo clippy` to ensure code meets quality standards.
- [x] 6.3 Verify the implementation via CLI manually by running collections with various `--type`, `--include`, and `--exclude` combinations, as well as providing `--account` and `--case` identifiers to observe the `manifest.json`.
