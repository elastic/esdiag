## 1. Dynamic API Key and Tag Extraction

- [x] 1.1 Expose `tags` property in the `Source` struct within `src/processor/diagnostic/data_source.rs`.
- [x] 1.2 Implement helper functions on the global sources `HashMap` to easily query all valid API keys for a product.
- [x] 1.3 Implement a helper function to return all API keys that contain a specific tag (e.g., `light`).

## 2. Refactoring API Resolver to Use Dynamic Strings

- [x] 2.1 Remove the `ElasticsearchApi` enum from `src/processor/api.rs` and update all usages to plain `String` identifiers.
- [x] 2.2 Update `es_base_apis` in `ApiResolver` to query the dynamic functions from `data_source.rs` instead of hardcoding `support` and `light` lists.
- [x] 2.3 Refactor `ApiResolver::resolve_es` to handle the `String` identifiers, including validation against the available `sources.yml` keys.

## 3. Implementing the Raw API Data Source

- [x] 3.1 Create a new module `src/processor/elasticsearch/raw_api.rs`.
- [x] 3.2 Define a `RawApi` struct that holds the dynamic `name` of the API.
- [x] 3.3 Implement the `DataSource` trait for `RawApi`, using its dynamic `name` field instead of a hardcoded string.
- [x] 3.4 Add an `execute_raw_api` helper method to `ElasticsearchDiagnostic` in `src/processor/elasticsearch/mod.rs` to fetch and save raw string outputs.

## 4. Parallel Collection Execution

- [x] 4.1 Update `ElasticsearchDiagnostic::process()` to capture the list of requested APIs that *don't* have a strongly-typed processor match.
- [x] 4.2 Spawn a new concurrent task (Thread 4) that uses `futures::stream::StreamExt::for_each_concurrent` (or similar) to iterate over the unhandled API keys.
- [x] 4.3 In the concurrent stream, instantiate `RawApi` for each key and execute it via the helper method.

## 5. Verification

- [x] 5.1 Run `cargo clippy --workspace --all-targets` and resolve any issues resulting from the enum deprecation.
- [x] 5.2 Execute `cargo test --workspace` to ensure dependency resolutions and URL pathings still function correctly.
- [x] 5.3 Run `cargo run -- collect localhost --type support` and visually verify the output directory matches parity with the legacy tools (large volume of JSON/TXT files).