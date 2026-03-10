## 1. Source Registry And Identifier Normalization

- [x] 1.1 Extend `src/processor/diagnostic/data_source.rs` to embed and expose `assets/logstash/sources.yml` under a Logstash product key
- [x] 1.2 Keep Logstash `DataSource` implementations product-agnostic while declaring canonical `logstash_*` aliases where needed
- [x] 1.3 Update `src/processor/api.rs` so Logstash include/exclude validation accepts canonical source keys and legacy short identifiers, then normalizes the execution plan to canonical keys
- [x] 1.4 Add unit tests covering Logstash source loading, version/path resolution, and canonical alias normalization
- [x] 1.5 Move source file and URL resolution into the active receiver or command product context so each execution uses a single product registry

## 2. Logstash Collection Pipeline

- [x] 2.1 Extend `src/processor/collector.rs` to dispatch `collect` for `Product::Logstash` instead of rejecting non-Elasticsearch products
- [x] 2.2 Implement a Logstash collector module that mirrors the existing retry, manifest, and archive-save flow used by the Elasticsearch collector
- [x] 2.3 Add typed save handling for `logstash_node` and `logstash_node_stats`, and raw save handling for the remaining Logstash `sources.yml` entries
- [x] 2.4 Ensure Logstash support collections expand from all `assets/logstash/sources.yml` keys while `minimal`, `standard`, and `light` preserve their bounded subsets
- [x] 2.5 Record the canonical collected Logstash API list and detected Logstash version in the diagnostic manifest
- [x] 2.6 Add dedicated Logstash client and receiver implementations for known-host collection instead of reusing Elasticsearch transport types

## 3. Verification

- [x] 3.1 Add coverage for Logstash support-profile expansion and duplicate-free alias normalization in collector or resolver tests
- [x] 3.2 Add coverage that `logstash_nodes_hot_threads_human` resolves and saves with the `.txt` extension
- [x] 3.3 Add ignored integration tests for externally managed Logstash `6.8.x`, `7.17.x`, `8.19.x`, and `9.x` instances
- [x] 3.4 Document the external-service assumptions or configuration needed to run the ignored Logstash compatibility tests
- [x] 3.5 Run `cargo clippy`
- [x] 3.6 Run `cargo test`
