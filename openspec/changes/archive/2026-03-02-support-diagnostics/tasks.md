## 1. Dynamic API Key and Tag Extraction

- [x] 1.1 Expose `tags` property in the `Source` struct within `src/processor/diagnostic/data_source.rs`.
- [x] 1.2 Implement helper functions on the global sources `HashMap` to easily query all valid API keys for a product.
- [x] 1.3 Implement a helper function to return all API keys that contain a specific tag (e.g., `light`).

## 2. Refactoring API Resolver to Use Dynamic Strings

- [x] 2.1 Enhance the `ElasticsearchApi` enum in `src/processor/api.rs` by adding a fallback `Raw(String, ApiWeight)` variant instead of deprecating it entirely.
- [x] 2.2 Update `es_base_apis` in `ApiResolver` to query the dynamic functions from `data_source.rs` instead of hardcoding `support` and `light` lists.
- [x] 2.3 Refactor `ApiResolver::resolve_es` to handle resolving dynamic keys into `ElasticsearchApi::Raw` fallback instances when no strongly-typed API matches.

## 3. Implementing the Raw API Data Source

- [x] 3.1 Implement a `get_raw_by_path` method on the `Receiver` to cleanly fetch string bodies directly based on resolved paths.
- [x] 3.2 Update `get_raw_by_path` to inject `Accept: text/plain` headers when the configured extension is `.txt`, ensuring `_cat` responses are preserved natively.
- [x] 3.3 Add a `save_raw` helper method to `ElasticsearchCollector` in `src/processor/elasticsearch/collector.rs` to fetch and stream un-parsed string outputs directly to disk.

## 4. Parallel Collection Execution

- [x] 4.1 Update `ElasticsearchCollector::collect()` to map the `Raw(String, ApiWeight)` variants into their respective `Light` vs `Heavy` execution paths correctly based on tag parsing.
- [x] 4.2 Stream the new lightweight generic APIs concurrently alongside the strongly typed APIs via the pre-existing `buffer_unordered(5)` implementation without needing a dedicated thread.
- [x] 4.3 Evaluate failure and logging states so missing node versions or mismatched clusters do not crash the concurrent streams.

## 5. Verification

- [x] 5.1 Run `cargo clippy --workspace --all-targets` and resolve any issues resulting from the new API enum.
- [x] 5.2 Execute `cargo test --workspace` to ensure dependency resolutions and URL pathings still function correctly.
- [x] 5.3 Run `cargo run -- collect localhost --type support` and visually verify the output directory matches parity with the legacy tools (large volume of JSON/TXT files).