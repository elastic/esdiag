## 1. Shared Source Model

- [x] 1.1 Refactor the shared source configuration types to deserialize both string-valued and structured version entries with `url`, `spaceaware`, and `paginate` metadata.
- [x] 1.2 Expand source initialization and lookup helpers to load both embedded Elasticsearch and Kibana catalogs in a product-keyed cache.
- [x] 1.3 Add unit tests covering Elasticsearch compatibility plus Kibana version resolution, metadata extraction, and file path generation.

## 2. Kibana API Planning

- [x] 2.1 Implement Kibana-specific API resolution that validates include/exclude values against `assets/kibana/sources.yml`.
- [x] 2.2 Add Kibana diagnostic type mappings so `minimal` resolves bootstrap APIs and `standard`, `support`, and `light` resolve the full Kibana source catalog.
- [x] 2.3 Encode Kibana required dependencies, especially `kibana_status` for version discovery and `kibana_spaces` for space-aware collection, and test the resolver behavior.

## 3. Kibana Collection Execution

- [x] 3.1 Implement the `src/processor/kibana` collection workflow and wire `Product::Kibana` through diagnostic construction and execution.
- [x] 3.2 Add raw Kibana request execution that expands `spaceaware` endpoints per discovered space and continues paginated endpoints until all pages are collected.
- [x] 3.3 Persist Kibana outputs using source-defined file paths plus scope-specific path segments for per-space and per-page artifacts, and report partial failures without aborting the full run.

## 4. Verification

- [x] 4.1 Add representative integration or workflow tests covering plain, space-aware, and paginated Kibana endpoints.
- [x] 4.2 Add ignored external-service compatibility tests for Kibana `6.8.x`, `7.17.x`, `8.19.x`, and `9.x`.
- [x] 4.3 Run `cargo clippy --workspace --all-targets`.
- [x] 4.4 Run `cargo test --workspace`.
