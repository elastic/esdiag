## 1. Struct and Logic Updates

- [x] 1.1 Update `EnrichedTask` struct in `src/processor/elasticsearch/tasks/processor.rs` to make the `node` field `Option<NodeDocument>`.
- [x] 1.2 Update the `EnrichedTask::new` constructor signature and implementation to accept `Option<NodeDocument>`.
- [x] 1.3 Modify the `documents_export` implementation in `src/processor/elasticsearch/tasks/processor.rs` to remove the `.expect()` call and handle the missing node case.
- [x] 1.4 Add a warning log message in `documents_export` when a node ID cannot be found in the lookup table.

## 2. Verification

- [x] 2.1 Run `cargo clippy` to ensure code quality and adherence to idioms.
- [x] 2.2 Run `cargo test` to verify that existing tests pass and no regressions were introduced.
- [x] 2.3 Verify that the changes compile and handle missing node metadata correctly (can be tested manually or by adding a new unit test if feasible).
