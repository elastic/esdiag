## 1. Setup

- [x] 1.1 Enable the `raw_value` feature for the `serde_json` dependency in `Cargo.toml`.

## 2. Refactoring Models

- [x] 2.1 Refactor `src/processor/elasticsearch/nodes_stats/data.rs` to replace `Option<serde_json::Value>` with `Option<Box<serde_json::value::RawValue>>`.
- [x] 2.2 Refactor `src/processor/elasticsearch/indices_stats/processor.rs` and `data.rs` to replace `Option<serde_json::Value>` with `Option<Box<serde_json::value::RawValue>>`.
- [x] 2.3 Refactor `src/processor/elasticsearch/cluster_settings/data.rs` and `processor.rs` to handle `RawValue` gracefully (especially if using `get` or manipulating settings).
- [x] 2.4 Refactor `src/processor/elasticsearch/mapping_stats/data.rs` to replace `Value` with `Box<RawValue>`.
- [x] 2.5 Refactor `src/processor/elasticsearch/tasks/` to replace `Value` with `Box<RawValue>`.
- [x] 2.6 Refactor `src/processor/elasticsearch/nodes/` to replace `Value` with `Box<RawValue>`.
- [x] 2.7 Refactor `src/processor/elasticsearch/slm_policies/` and `ilm_policies` to replace `Value` with `Box<RawValue>`.
- [x] 2.8 Review all remaining `serde_json::Value` occurrences in `src/processor/elasticsearch/` and replace with `Box<RawValue>` if they are pass-through flexible schemas.
- [ ] 2.9 Review all `serde_json::Value` occurrences in `src/processor/logstash/` and replace with `Box<RawValue>`.

## 3. Verification

- [ ] 3.1 Run `cargo clippy` and `cargo fmt` to ensure code quality standards.
- [ ] 3.2 Run `cargo test` to ensure all streaming deserialization tests pass with the new `RawValue` struct layout.
- [ ] 3.3 Spin up `esdiag serve` and perform a functional test via `/upload/submit` using `tests/archives/elasticsearch-api-diagnostics-8.19.3.zip` to guarantee serialization outputs remain valid NDJSON.